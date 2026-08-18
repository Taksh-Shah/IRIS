# BLE-002 VERIFICATION — BLE Transport (iOS) AC-1..16 evidence + verifier verdict

**Document ID**: IRIS-BLE-002-VERIF-001
**Date**: 2026-08-18
**Stage**: VERIFY (iter ~138) + verifier APPROVE (iter ~139) → ACCEPT (iter ~140)
**Pattern**: BLE_001_VERIFICATION.md / ANDROID-001_VERIFICATION.md

---

## Verifier verdict

- **Independent verifier pass 1 (iter ~138)**: reproduced runtime evidence —
  `cargo test -p iris-core transport::ble` **56/0** (1.57s live re-run),
  workspace **647/0/1** (all-features), clippy **0**, fmt **clean**, every
  BLE-RT-C00x disposition present (FIXED → passing regression, RECORDED →
  honest claim). Verdict **FAIL (doc-only)** → **NEEDS_FIX**, 6 evidence-
  integrity findings (VR-01..VR-06) — all **resolved in-pass by supervisor**
  (supports_identify_read purge; fake test names → actual; AC-5 real test name;
  iter 137 logged; context refresh; baseline 646→647 + BE-RT typo).
- **Independent verifier pass 2 (iter ~139)**: confirmed every VR-01..VR-06
  closure via fresh greps + the one documented residual
  (BLE-002_SECURITY_REVIEW.md:56 `646/0/1`) fixed and confirmed zero-match;
  live `transport::ble` re-run **56/0**. Verdict: **APPROVE**.
- No code changes were required at VERIFY; the fixes were purely coercive
  record corrections. Runtime evidence held throughout.

## AC → evidence table

| AC | Verdict | Evidence |
|----|---------|----------|
| AC-1 | PASS | `ios_leg_registers_and_is_selectable_via_transport_manager` — BleTransport registers via TransportManager + selectable as iOS leg; `capability_matrix_marks_ios_background_limited` pins `supports_background_ios: false`. BLE-001 registration base held. |
| AC-2 | PASS | `ios_mtu_payload_sizes_frames_to_full_reported_budget` — 23-B default + 20-B degraded guard → correct ATT budget; `ios_connect_to_identify_yields_candidate_from_identify_characteristic` exercises 512-B-bound identify payload. `ble_att::payload_to_mtu` (payload+5→MTU, cap 512, 0→negotiated ceiling, ≥20 accepted; DEC-BLE-002-0004). |
| AC-3 | PASS | `ios_connect_to_identify_yields_candidate_from_identify_characteristic` — UUID-only iOS ad → connect → `gatt_read(IRIS_IDENTIFY_CHARACTERISTIC)` → `DiscoveryBeacon::parse` → candidate; disconnect after read. `android_and_ios_carriers_share_one_beacon_parser` (same beacon, both carriers). `c001_android_leg_never_probe_connects_empty_ads` (BLE-RT-C001 regression) — 0 candidates + 0 connects on empty ads. stop_discovery held. |
| AC-4 | PASS | E2E connect/send/receive via SimulatedBleAdapter held; connect-to-identify carrier + `c005_sim_serves_advertised_beacon_on_identify_read` (advertise→read→serve handoff). `transport::ble` = 56/0. |
| AC-5 | PASS | `scan_burst_is_throttled_within_window` (ble.rs:1087) + error-propagation held; service-UUID filter mandatory (`ScanFilter.service_uuids` non-empty enforced at scan start). Previously recorded `scan_restart_backoff_*` does not exist — corrected VR-04. |
| AC-6 | PASS | `ble_advert` adversarial property/regression tests (malformed/truncated/oversized — no panic/blowup) on both carriers via `ios_connect_to_identify...` + `malformed_gatt_writes`. |
| AC-7 | PASS | Connect-to-read yields candidates only; every inbound payload flows through the engine envelope-verify seam (CRYPTO-001/SEC-001) — no bypass on read path. Redteam dispositioned at SECURITY_REVIEW (AC-15). |
| AC-8 | PASS | Lifecycle: shutdown → poller stop + disconnect + Unavailable (BLE-001 RED-0009 regressions held). `gatt_read` + identify characteristic are pure trait ops (no BLE init on Rust side; iOS init = IOS-001/IosBleAdapter). |
| AC-9 | PASS | Per-peer single GATT connection reuse + prompt close held; ~8-peer bound via `max_peers` (BLE-001 BLE-RT-013); iOS ceiling documented (RES-0024 RQ-3). |
| AC-10 | PASS | docs/transports/BLE.md §iOS + docs/platforms/IOS.md carry both UIBackgroundModes, overflow area (iOS-iOS-only), no PendingIntent analog, willRestoreState restart, system-kill vs force-quit, Live Activity screen-on only, `supports_background_ios: false`, min patch floor, misattributed CVE exclusion set (RES-0024 applied). |
| AC-11 | PASS | `BleAdapter` trait doc-updated for IosBleAdapter semantics (gatt_read = CoreBluetooth `readValue(for:)`); SimulatedBleAdapter conformance type-checks + clippy-clean (0 warnings / 0 errors live). |
| AC-12 | PASS | BLE.md + IOS.md + TRANSPORT_ABSTRACTION.md + graph agree with code; RES-0024 outdated-claims table applied. |
| AC-13 | PASS | Live: `cargo test --workspace --all-features` → **647 passed / 0 failed / 1 ignored** (633 + 14 new: 11 BLE-002 + 3 BLE-RT); clippy **0**; fmt clean. Baseline documented 647/0/1 (VR-06 correction). |
| AC-14 | GATED | Physical-device CoreBluetooth connect/send/receive + battery BENCH = **known_limitation BLK-0005** (iOS Simulator does not support CoreBluetooth; real iPhone required). Recorded, not a blocker. |
| AC-15 | PASS | SECURITY_REVIEW (iter ~137): redteam FAIL → RESOLVED; 8 findings (BLE-RT-C001..C008) dispositioned in BLE-002_SECURITY_REVIEW.md — C001 HIGH FIXED (ios_leg gate + service-UUID filter + 8-probe budget + 2 regressions), C002 HIGH CORRECTED (evidence-integrity), C003/C004 MEDIUM mitigated/recorded → IOS-001/ANDROID-001, C005 MEDIUM FIXED (sim-serve honesty + regression), C006/C007/C008 handled. |
| AC-16 | PASS | This document: independent verifier APPROVE (iter ~139) after VR-01..VR-06 closure. |

## Findings dispositioned at VERIFY

- **VR-01** — PROJECT_GRAPH:285 asserted nonexistent `supports_identify_read` flag → rewritten.
- **VR-02** — stale `supports_identify_read` in 7 durable records → correction-only phrasing.
- **VR-03** — NEXT_ACTION cited fake `ios_leg_discovery_no_connect_no_read` → actual test coverage named.
- **VR-04** — BLE-002_TEST.md AC-5 cited fake `scan_restart_backoff_*` → real `scan_burst_is_throttled_within_window` (ble.rs:1087).
- **VR-05** — iter 137 missing from records; context files stale → logged + refreshed.
- **VR-06** — baseline 646→647 (measured); "BE-RT"→"BLE-RT"; last residual (SECURITY_REVIEW.md:56) fixed at iter ~139.

## Next

ACCEPT (iter ~140): PROJECT_GRAPH BLE-002 status COMPLETE (evidence 7),
PROJECT_STATE completed 26→27, CHANGELOG, NODE_TRANSITION → **IOS-001** (DISCOVER, iter ~141).