# BLE-002 TEST — BLE Transport (iOS) AC-1..13 evidence map

**Document ID**: IRIS-BLE-002-TEST-001
**Date**: 2026-08-18
**Stage**: TEST (iter ~136) — **REVISED at SECURITY_REVIEW (iter ~137, redteam
FAIL→RESOLVED)**: AC-3/AC-2/AC-4 evidence rows corrected to ACTUAL test names
(record previously cited nonexistent `ios_leg_discovery_no_connect_no_read` +
`identify_peak_budget_and_roundtrip_pr_small_frame` — BLE-RT-C002); two-carrier
asymmetry is now enforced + proven by real `c001_android_leg_never_probe_connects_empty_ads` regression.
**Status**: COMPLETE — SECURITY_REVIEW (AC-15) → VERIFY (AC-16) → ACCEPT next
**Live re-run THIS pass**: transport::ble **56/0** (1.52s), workspace **647/0/1**
(all-features), clippy **0**, fmt **clean**. Baseline updated 644/0/1 → **647/0/1**.

---

## AC → evidence table

| AC | Verdict | Evidence |
|----|---------|----------|
| AC-1 | PASS | `transport::ble::ios_leg_registers_and_is_selectable_via_transport_manager` — BleTransport registers via TransportManager + is selectable as the iOS leg (capabilities/eligibility incl. `supports_background_ios: false`). BLE-001 base registration tests also held. |
| AC-2 | PASS | `ios_mtu_payload_sizes_frames_to_full_reported_budget` (23-B default + 20-B degraded guard → correct ATT payload budget) + `ios_connect_to_identify_yields_candidate_from_identify_characteristic` exercises the 512-B-bound identify payload. `ble_att::payload_to_mtu` maps `maximumWriteValueLength` payload+5 → MTU, cap 512, 0 → negotiated ceiling, ≥20 accepted (DEC-BLE-002-0004). Segmenter resize on MTU-change path held from BLE-001. |
| AC-3 | PASS | `ios_connect_to_identify_yields_candidate_from_identify_characteristic` — UUID-only iOS ad (empty payload) → connect → `gatt_read(IRIS_IDENTIFY_CHARACTERISTIC)` → `DiscoveryBeacon::parse` → candidate peer; disconnect after read. `android_and_ios_carriers_share_one_beacon_parser` — same beacon produced via ad (Android) and characteristic (iOS). **`c001_android_leg_never_probe_connects_empty_ads`** (BLE-RT-C001 regression, iter ~137) — Android leg yields 0 candidates + **0 connects** on empty-payload ads (two-carrier asymmetry enforced). `stop_discovery` path held from BLE-001 AC-3. |
| AC-4 | PASS | E2E connect/send/receive over `SimulatedBleAdapter` held from BLE-001; connect-to-identify carrier covered by `ios_connect_to_identify_yields_candidate_from_identify_characteristic` + `c005_sim_serves_advertised_beacon_on_identify_read` (advertise→read→serve handoff). `cargo test -p iris-core transport::ble` = **56/0** total. |
| AC-5 | PASS | Scan-burst throttling (`scan_burst_is_throttled_within_window`) + error-propagation tests held from BLE-001 AC-5 (real test name confirmed via `cargo test --list` — the previously recorded `scan_restart_backoff_*` does not exist; VR-04 correction iter ~138). Service-UUID filter mandatory = `ScanFilter.service_uuids` non-empty enforced at scan start (iOS contract); existing discovery tests assert filter round-trip. |
| AC-6 | PASS | `ble_advert` adversarial property/regression tests (malformed/truncated/oversized beacons — no panic/alloc blowup) held from BLE-001 AC-6; same `DiscoveryBeacon::parse` core exercised on the characteristic-carried path via `ios_connect_to_identify...` + `malformed_gatt_writes` regression — both carriers reject without panic. |
| AC-7 | PASS | Code-review assertion: connect-to-read path yields a **candidate** only (no payload triggers); every inbound payload still flows through the engine envelope-verify seam (CRYPTO-001/SEC-001) — no new bypass on the read path. BLE-001 AC-7 security tests held. Full redteam disposition at SECURITY_REVIEW (AC-15). |
| AC-8 | PASS | Lifecycle: `shutdown()` stops poller + disconnects + Unavailable (BLE-001 RED-0009-01/02 regression held). Restoration-seam types: `gatt_read` + identify characteristic are pure trait ops (no BLE init on the Rust side; iOS init is IOS-001/IosBleAdapter scope). `no BLE init outside valid launch path` = documented IOS-001 contract (RES-0024 DI-6). |
| AC-9 | PASS | Per-peer single GATT connection reuse + `close()` promptly held from BLE-001 AC-9 (connection-reuse tests). ~8-peer bound: `max_peers` cap enforced (BLE-001 `BLE-RT-013`); iOS practical ceiling documented in graph known_limitations + RES-0024 RQ-3. |
| AC-10 | PASS | Background contract DOCUMENTED (RES-0024 outdated-claims table applied): docs/transports/BLE.md §iOS (lines 254-360) + docs/platforms/IOS.md carry both UIBackgroundModes, overflow-area (iOS-iOS-only), no PendingIntent analog, state restoration re-start-in-willRestoreState, system-kill vs user-force-quit, Live Activity screen-on only, `supports_background_ios: false` (ble.rs:287 + test `capability_matrix_marks_ios_background_limited`), min iOS patch floor, misattributed CVE exclusion (CVE-2023-28412/2024-44270/2021-31714 excluded; verified set = CVE-2023-42941/2024-23241/2024-44124/2024-44191 OS-patchable). |
| AC-11 | PASS | `BleAdapter` trait doc-updated with IosBleAdapter semantics (gatt_read contract for CoreBluetooth `readValue(for:)`); `SimulatedBleAdapter` conformance type-checks + clippy-clean. `cargo clippy --workspace --all-targets` **0 warnings / 0 errors** live. |
| AC-12 | PASS | Doc reconciliation: BLE.md + IOS.md + TRANSPORT_ABSTRACTION.md + BLE-002 node agree with code; RES-0024 outdated-claims table applied in BLE_002_DESIGN.md (1-2% duty cycle cited L5; iOS 26 accessory modes not general relaxation). No drift found in this pass. |
| AC-13 | PASS | **Live re-run** (iter ~137): `cargo test --workspace --all-features` → **647 passed / 0 failed / 1 ignored**; `cargo clippy --workspace --all-targets` → 0 warnings; `cargo fmt --all --check` → clean. BLE-001 `transport::ble` 42 tests held (now 56 = 42 + 11 BLE-002 + 3 new BLE-RT regressions). Baseline updated 644/0/1 → 647/0/1. |
| AC-14 | GATED | Physical-device CoreBluetooth connect/send/receive + battery BENCH = **known_limitation BLK-0005** (iOS Simulator does not support CoreBluetooth; real iPhone required). Recorded, not a blocker. |
| AC-15 | PENDING | SECURITY_REVIEW stage (next) — redteam adversarial findings dispositioned/fixed/recorded. |
| AC-16 | PENDING | VERIFY stage (after) — independent verification doc + verifier APPROVE. |

## Stage notes

- `cargo test -p iris-core transport::ble` — **56 passed / 0 failed** (was 42 at
  BLE-001; +11 BLE-002 tests + 3 SECURITY_REVIEW regressions: c001_android_leg_never_probe_connects_empty_ads,
  c001_ios_leg_probe_budget_caps_probe_connects, c005_sim_serves_advertised_beacon_on_identify_read).
- Workspace total **647/0/1** = 633 pre-BLE-002 baseline + 14 new tests (11 BLE-002 + 3 BLE-RT regressions).
- **SECURITY_REVIEW REDTEAM (iter ~137) FAIL → RESOLVED**: BLE-RT-C001 (HIGH —
  Android leg probe-connecting ambient empty ads) FIXED (gate on `ios_leg` +
  IRIS UUID filter + 8-probe budget + 2 regression tests); C002 (HIGH — records
  cited nonexistent tests) CORRECTED to actual test names + real regression
  coverage; C003 (MEDIUM — probe timeouts) contract item → IOS-001, probe
  budget applied; C004 (MEDIUM — probe cache) RECORDED → IOS-001 follow-up;
  C005 (MEDIUM — sim serve honesty) FIXED (start_advertising→identify_data
  handoff + regression test); C006 (LOW — payload_to_mtu(0) ceiling) noted
  RECORDED; C007 (LOW — drain collect) one-line FIXED; C008 (INFO — probe
  errors) RECORDED + instrumented.
- AC-14 GATED is recorded in PROJECT_GRAPH known_limitations (BLK-0005) per
  ACCEPTANCE_POLICY — physical device path remains an explicit, documented
  gate, not a silent skip.

## Next

VERIFY (iter ~138, AC-16): independent verifier reproduces workspace
647/0/1 + clippy 0 + fmt clean + all BLE-RT dispositions present → ACCEPT
(iter ~139) → IOS-001.