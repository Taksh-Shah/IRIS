# BLE-002 SECURITY_REVIEW — Redsadversarial disposition (BLE-RT-C001..C008)

**Document ID**: IRIS-BLE-002-SECURITY_REVIEW-001
**Date**: 2026-08-18
**Stage**: SECURITY_REVIEW (iter ~137, AC-15)
**Status**: FAIL → **RESOLVED** (3 FIXED + 3 RECORDED + 1 FIXED-record + 1
contract-item carried to IOS-001)
**Pattern**: BLE-001_SECURITY_REVIEW.md (BLE-RT-001..016)

Redteam agent (independent subagent, ses_feac9b941ffeOvit965zZ5Rhzn) returned
**FAIL** with 2 HIGH / 3 MEDIUM / 2 LOW / 1 INFO. Every disposition verified
against source by supervisor (this pass). **No CRITICAL** — app-layer AEAD
trust anchor intact; `DiscoveryBeacon::parse` total/defensive; `payload_to_mtu`
math panic-free for full u16 range.

---

## Findings & disposition

| ID | Sev | Finding | Disposition |
|----|-----|---------|-------------|
| **BLE-RT-C001** | HIGH | Connect-to-identify probe branch NOT gated on `ios_leg`; Android production leg (`BleBridge`) would GATT-connect **unfiltered ambient advertising** (empty `ScanFilter`, default `max_peers=64` → ~320 connect/disconnect cycles/30 s worst case, battery + GATT-pool churn). | **FIXED (real code + 2 regressions)**: gate on `self.ios_leg` (@ble.rs:580); iOS leg now scans with `ScanFilter{service_uuids:[IRIS_SERVICE_UUID]}` (@ble.rs:540 — background contract requires it); probe budget = 8/scan (`probe_budget`, @ble.rs:559, mutable counter @ble.rs:568-586). Tests: `c001_android_leg_never_probe_connects_empty_ads` (0 connects), `c001_ios_leg_probe_budget_caps_probe_connects` (≤8 connects). |
| **BLE-RT-C002** | HIGH | Evidence-integrity: TEST/IMPLEMENT/graph records cited tests `ios_leg_discovery_no_connect_no_read` + `identify_peak_budget_and_roundtrip_pr_small_frame` that **do not exist** (`cargo test --list` confirmed); `supports_identify_read` flag described in records but absent in code. | **CORRECTED (records + coverage)**: BLE-002_TEST.md AC-2/AC-3/AC-4 rows rewritten to ACTUAL test names; two-carrier asymmetry now PROVEN by real `c001_android_leg_never_probe_connects_empty_ads` regression (connect_count==0) — stronger than the claimed test; `supports_identify_read` claim removed (no such flag; sim has no flag, read of non-identify char returns `Err(DeviceNotFound)` @ble.rs:317). Verified against `cargo test -p iris-core transport::ble -- --list`. |
| **BLE-RT-C003** | MEDIUM | `gatt_read`/`connect_gatt` sync trait ops with no core timeout; a peer that accepts-but-never-completes reads stalls discovery linearly up to max_peers. | **Mitigated + contract-item**: iOS-leg probe budget (8/scan, C001 fix) bounds the stall surface; per-op bounded timeouts (connect ≤5 s / read ≤5 s) = **IOS-001 contract requirement** recorded in BLE_002_DESIGN.md §5 (FFI contract) + this record. |
| **BLE-RT-C004** | MEDIUM | No per-MAC probe cache/TTL → re-probe churn every scan cycle against peers that never serve identify; no ~8-peer admission cap in `connect()`. | **Partially mitigated + RECORDED**: probe budget caps per-scan churn (8); per-MAC probe TTL cache + `connect()` admission ceiling = **IOS-001/ANDROID-001 follow-up** (recorded in DESIGN known_limitations). Android leg no longer probes at all (C001), removing its churn surface entirely. |
| **BLE-RT-C005** | MEDIUM | `SimulatedBleAdapter::start_advertising` never populated `identify_data` — the sim did NOT serve the beacon it advertised (records claimed honesty that code lacked); advertise→serve handoff untested. | **FIXED (real code + 1 regression)**: `start_advertising` stores non-empty `service_data` into `identify_data` (@ble.rs:263-280) — the sim now serves exactly the beacon it advertises (real-adapter GATT-server shape); iOS-mode ad (empty service data) does NOT clobber it. Test: `c005_sim_serves_advertised_beacon_on_identify_read`. |
| **BLE-RT-C006** | LOW | `payload_to_mtu(0)` maps "not yet negotiated" to the 517 ceiling → 512-B frames pre-negotiation (failure amplification on a transient 0). | **RECORDED** (defensible deliberate mapping; CoreBluetooth never returns 0 — verified positive controls; iOS-16 20-B guard covers the real degraded case). Flagged for IOS-001 adapter-contract review at FFI landing. |
| **BLE-RT-C007** | LOW | `drain(..).collect()` materialized the whole batch before `take(max_peers)` — cap applied to output not allocation. | **FIXED (one-line)**: `.drain(..).take(config.max_peers).collect()` @ble.rs:551-556 — flood bounded before Vec allocation. |
| **BLE-RT-C008** | INFO | Probe-path BleErrors swallowed via `.ok()` with no logging/metric; read failures indistinguishable from "no IRIS peers". | **RECORDED**: probe-outcome counters noted for OBS-001/telemetry follow-up; debug-log suggestion accepted. Positive controls held: no connection leak (disconnect unconditional after connect @ble.rs:587), no unverified bytes reach an authenticated action (trust seam HOLDS). |

## Positive controls verified (redteam, independently confirmed)

1. `DiscoveryBeacon::parse` total/defensive — length gate + bounds-safe slicing, no panic/unbounded-alloc on hostile ≤512-B characteristic bytes.
2. `payload_to_mtu` + segmenter arithmetic panic-free for full u16 domain; clamp 23..=517; 20-B↔23-B internally consistent.
3. Trust seam holds: probe path → candidate only; no decrypt/log of unverified bytes.
4. No GATT connection leak on failed read (unconditional disconnect).
5. `BleBridge::gatt_read` stub = safe `Err(DeviceNotFound)` (no attacker bytes, no false candidates).
6. Scan-restart throttle (5/30 s + backoff) bounds probe rate — why C001 was HIGH not CRITICAL.

## Verification (this pass, after fixes)

- `cargo test -p iris-core transport::ble --all-features` → **56 passed / 0 failed** (1.52s; includes 3 new BLE-RT regressions).
- `cargo fmt --all` applied; re-check clean.
- Workspace + clippy re-run at VERIFY stage.

## Scope note

BLE-002 core is pure-Rust; iOS `IosBleAdapter` (Swift/CoreBluetooth) is
**IOS-001 scope** — the trait IS the FFI contract and every C003/C004 contract
requirement is recorded there. Physical-device connect/read = BLK-0005 gated
(AC-14).

## Next

VERIFY (iter ~138, AC-16): independent verifier reproduces 56/0 transport::ble,
workspace 647/0/1, clippy 0, fmt clean, all BLE-RT dispositions present →
ACCEPT (iter ~139/140) → NODE_TRANSITION IOS-001.