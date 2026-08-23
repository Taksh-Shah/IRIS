# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-22T14:00:00Z

---

## CURRENT STATE: MISSION GRAPH COMPLETE — AWAITING PHYSICAL PHONES

**32 of 33 nodes COMPLETE.** LEGAL-001 DEFERRED_TO_FUTURE (carried
obligations register at `docs/legal/CARRIED_OBLIGATIONS_REGISTER.md`).
Baseline **727/0/1**, clippy 0, fmt clean, audit exit 0.

Full test report: `docs/testing/SYSTEM_TEST_REPORT.md`

---

## PHYSICAL-PHONE TESTING PHASE — Setup Instructions for Operator

### What you need to provide

| Item | Details |
|------|---------|
| 2+ Android phones | Old/repurposed is fine — Android 8.0+ (API 26+) required; BLE + Wi-Fi Direct hardware |
| USB cables | For `adb` connection from your PC |
| Developer mode | Enable on each phone: Settings → About → tap Build Number 7× → Developer Options → enable USB Debugging |
| Your PC | Windows with USB ports; we'll install the Android SDK toolchain |

### Is a 3rd phone useful?

**YES — strongly recommended.** With 2 phones you can prove direct P2P
(BLE discovery + message delivery). With 3 phones you unlock **true multi-hop
mesh**: Phone A sends → Phone B relays (out-of-range of A) → Phone C
receives. This exercises the DTN store-carry-forward code path on real
hardware and proves the mesh actually meshes.

### Build environment setup (I will guide step-by-step when you're ready)

1. Install Android Studio (or just command-line tools) → gets you SDK + platform tools
2. Install Rust Android targets: `rustup target add aarch64-linux-android armv7-linux-androideabi`
3. Install cargo-ndk: `cargo install cargo-ndk`
4. I build the APK: `cargo ndk -t arm64-v8a -p iris-android ... && gradlew assembleDebug`
5. You connect phones via USB → `adb install` on each

### What we'll test on real hardware

| Test | Phones needed | What it proves |
|------|--------------|----------------|
| BLE discovery | 2 | One phone advertises IRIS service, other discovers it |
| BLE GATT delivery | 2 | Message sent from A arrives at B over BLE |
| Wi-Fi Aware discovery | 2 (both must support NAN) | NDP session formation |
| Wi-Fi Direct group | 2 | P2P group formation + TCP delivery |
| Multi-hop relay | **3** | A→B→C where B is out of BLE range of A |
| SOS priority | 2–3 | P0 bypasses all queues, arrives first |
| Background behaviour | 2 | App survives screen-off, Doze mode |
| Battery drain | 2 | Measured vs estimated (EXP-003) |
| OEM battery-kill | 2 | Verify whitelist guidance per OEM matrix |
| Reconnect resilience | 2 | Toggle Bluetooth/Wi-Fi mid-session; auto-recovery |

---

## Pre-existing work: nothing pending in the software graph

All engineering nodes are COMPLETE or DEFERRED_TO_FUTURE. The only remaining
work items are:
1. Physical-device testing (this phase)
2. LEGAL-001 counsel engagement (deployment-time)
3. Mission milestone planning (MVP/Alpha/Beta/Pilot)

---

## Recent commits

```
80f0091 SAT-001 post-accept bookkeeping
109bf09 SAT-001 post-accept hygiene: FC-1..FC-8 SATELLITE.md corrections
15bfdf4 supervisor: HUMAN_GATE set at graph exhaustion
91d6924 SYSVAL-001 whole-system validation COMPLETE + LEGAL-001 deferred
```
