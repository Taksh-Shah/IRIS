# ACTIVE NODE

**Schema version**: 1.0
**Last updated**: 2026-08-17T05:00:00Z

## Active Node: ANDROID-001 - Android Platform Integration

- **Type**: PLATFORM (Android app shell + Kotlin FFI adapters)
- **Priority**: P0
- **Status**: DISCOVERED **UNDERSTAND COMPLETE (iter 110)** -> RESEARCH (iter 111)
- **Deps**: BLE-001, WIFIAWARE-001, MSG-001, EMERG-001 - ALL COMPLETE
  (critical path -> PILOT-001)
- **C2 gap**: NO acceptance_criteria -> DESIGN (iter 113) defines AC-1..n

## UNDERSTAND COMPLETE (iter 110)

- Read PROJECT_GRAPH node + **docs/platforms/ANDROID.md** (304 lines) +
  **docs/implementation/KOTLIN_LAYER.md** (397 lines): Kotlin/Compose + Hilt MVVM
  + FGS (connectedDevice API 34+) + WorkManager + OEM battery-kill matrix +
  Keystore ECDSA + Room/SQLCipher metadata-only + UniFFI IrisCore.kt + cargo-ndk
  (arm64-v8a/armeabi-v7a/x86_64, generated Kotlin committed).
- **Reference embedding pattern**: DESKTOP-001 `DesktopEngine`
  (`crates/iris-desktop/engine_handle.rs`) - TransportManager + register
  transports + MessageEngine + auto inbox forwarder. Mirror in Android with
  Kotlin adapter injection (BleAdapter 10-op sync ble.rs:122; WifiAwareAdapter
  12-op async wifiaware.rs:156; WifiDirectAdapter 20-op async wifi_direct.rs:180).
- **Net-new scaffolding**: no `android/`/`kotlin/` dir, no `.udl`/uniffi yet.
- **FFI-contract to consume**: NEW-WA-RT-108 (verified-peer reuse key), 109
  (closed-NDP prune + multi-subscriber stream), 110 (per-call timeouts), 111
  (start/subscribe idempotence), 112 (ring-buffer outbox) + WIFIDIRECT RT-010
  (start/start_dns_sd idempotence).
- **C2 AC-gap confirmed**; graph evidence(7)/known_limitations(6)/stage_note set;
  status held DISCOVERED. Workspace untouched: **608/0/1 clippy 0** held.

## Next Action (ANDROID-001 RESEARCH, iter 111)

1. Research 2026 Android platform SOTA for the FFI-contract + app-shell
   questions (RES-0022): UniFFI callback-interface support for async Rust
   traits / mechanical bridge for injecting Kotlin adapters; UniFFI vs manual
   JNI + cargo-ndk build (NDK r26+, targets, committed-generated-Kotlin).
2. Kotlin 2.x/Compose 2026 app shell best practice; FGS connectedDevice
   (API 34+) + WorkManager cadence + OEM battery-kill handling.
3. Keystore hardware-backed ECDSA signing vs Rust Ed25519 session keys
   (IDENT-001 RED-0005 binding).
4. BLE/Wi-Fi Aware/Wi-Fi Direct 2026 API status re-verify (target/compile SDK).
5. Record RES-0022 (evidence-primary sources, L1-L5 maturity). Workspace
   untouched (baseline held) -> DESIGN (iter 113).

## Context

- **24 COMPLETE nodes** (WIFIDIRECT-001 iter 109, WIFIAWARE-001 iter 101,
  BLE-001 iter 90). DISCOVERED: BLE-002, LORA-001, SAT-001, TEST-001,
  ANDROID-001, IOS-001, PILOT-001.
- Critical path: ... -> EMERG-001 -> **ANDROID-001** -> PILOT-001.
- Transport pipeline complete: INTERNET-001 -> BLE-001 -> WIFIAWARE-001 ->
  WIFIDIRECT-001. P2 transports LORA-001/SAT-001 deferred (iter 109b).
- Workspace: **608 passed / 0 failed / 1 ignored**, clippy **0**.

## Related Records

- Platform reference: DESKTOP-001 (Tauri v2 shell hosting iris-core = mirror)
- Transports consumed: BLE-001, WIFIAWARE-001, WIFIDIRECT-001, INTERNET-001
- Carry-forward: NEW-WA-RT-108..112 + WIFIDIRECT RT-010 (Android FFI contract)
- Gates: DEC-0009 (implementation authorized) - BLK-0005 (device tests gated)