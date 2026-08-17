# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-17T05:00:00Z

---

## Priority: ANDROID-001 RESEARCH - Android Platform Integration (iter 111)

ANDROID-001 **UNDERSTAND COMPLETE (iter 110)**: read PROJECT_GRAPH node (P0
PLATFORM, deps BLE-001/WIFIAWARE-001/MSG-001/EMERG-001 all COMPLETE) +
`docs/platforms/ANDROID.md` (304 lines) + `docs/implementation/KOTLIN_LAYER.md`
(397 lines). Reference embedding pattern DESKTOP-001 DesktopEngine
(`crates/iris-desktop/engine_handle.rs`). FFI-contract inventoried
(NEW-WA-RT-108..112 + WIFIDIRECT RT-010 idempotence). **C2 AC-gap confirmed**
(no acceptance_criteria) -> DESIGN (iter 113) defines AC-1..n. Graph
evidence(7)/known_limitations(6)/stage_note set. Baseline **608/0/1 clippy 0**
held. **NODE_TRANSITION CORRECTED (iter 109b)**: ANDROID-001 (P0, P0-first)
selected over LORA-001 (P2 hardware-gated, deferred).

## Next Action (ANDROID-001 RESEARCH, iter 111, RES-0022)

2026 Android platform SOTA for FFI-contract + app-shell questions:
1. **UniFFI callback-interface support for async Rust traits** — pin the
   mechanical bridge for injecting Kotlin adapters into iris-core (BleAdapter
   10-op sync / WifiAwareAdapter + WifiDirectAdapter async traits reusing
   async_trait). PRIMARY open question — callbacks for `incoming_*`, streams,
   availability push.
2. **UniFFI vs manual JNI for the engine surface + cargo-ndk build**
   (libiriscode.so, NDK r26+, targets arm64-v8a/armeabi-v7a/x86_64,
   committed-generated-Kotlin convention per ANDROID.md); re-verify 2026
   current versions (uniffi crate, rust-android-gradle).
3. **Kotlin 2.x/Compose 2026 app-shell best practice** (ANDROID.md pins
   Kotlin 1.9+/Compose BOM 2024.01.00) + FGS connectedDevice (API 34+) +
   WorkManager cadence + OEM battery-kill matrix UX.
4. **Keystore hardware-backed ECDSA signing vs Rust Ed25519 session keys**
   (IDENT-001 RED-0005 binding; StrongBox).
5. **BLE/Wi-Fi Aware/Wi-Fi Direct 2026 Android API status re-verify**
   (target/compile SDK 34 vs current 36/37, behavior changes).
Record RES-0022 (pattern RES-0019/0020/0021: evidence-primary sources,
L1-L5 maturity, no AI citations) + ALLOCATION (next RES-0023). Workspace
untouched (baseline 608/0/1 held) -> DESIGN.

## Context

- **Active node ANDROID-001** (P0 PLATFORM); deps ALL COMPLETE. After:
  TEST-001 -> IOS-001 / PILOT-001. P2 transports (LORA-001/SAT-001) deferred.
- **24 COMPLETE nodes** (WIFIDIRECT-001 iter 109, WIFIAWARE-001 iter 101,
  BLE-001 iter 90).
- Workspace: **608 passed / 0 failed / 1 ignored** (all-features), clippy **0**.
- Critical path: ... -> EMERG-001 -> **ANDROID-001** -> PILOT-001 (MVP demo
  milestone: PROTO-001/CRYPTO-001/BLE-001/MSG-001/ROUTE-001/ANDROID-001).

## Dependencies (must be COMPLETE)

- ANDROID-001 deps: BLE-001, WIFIAWARE-001, MSG-001, EMERG-001 - ALL COMPLETE.
  Reference: DESKTOP-001 (Tauri v2 shell hosting iris-core) = embedding pattern.

## State Transition After Completion

- ANDROID-001: UNDERSTAND(110, complete) -> RESEARCH(111, RES-0022) ->
  DESIGN -> IMPLEMENT -> TEST -> SECURITY_REVIEW -> VERIFY -> ACCEPT ->
  PILOT-001.

## Related Records

- Platform reference: DESKTOP-001 (DESKTOP_VERIFICATION.md v1.0, Tauri shell)
- Transports consumed: INTERNET-001, BLE-001, WIFIAWARE-001, WIFIDIRECT-001
- Carry-forward: NEW-WA-RT-108..112 + RT-010 (Android FFI contract)
- Gates: DEC-0009 (implementation authorized) - BLK-0005 (device tests gated)