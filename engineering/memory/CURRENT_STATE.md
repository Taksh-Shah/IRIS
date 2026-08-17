# CURRENT_STATE.md

**Schema version**: 1.0
**Last updated**: 2026-08-17T04:00:00Z

---

## Summary

All core-engine + routing + observability work packages (WP-A codec → WP-9
OBS-001), the desktop platform shell (WP-10 DESKTOP-001), the ML routing
experiments (ML-001), the cryptographic layer (CRYPTO-001), the identity
system (**IDENT-001**), the emergency system (**EMERG-001**), the general
security hardening (**SEC-001**), and the first three transports (**BLE-001**,
**WIFIAWARE-001**, **WIFIDIRECT-001**) are implemented, tested, verified, and
accepted.
GW-001, ROUTE-002, OBS-001, DESKTOP-001, REQ-001, ARCH-001, ML-001,
CRYPTO-001, **IDENT-001**, **EMERG-001**, **SEC-001**, **BLE-001**, and
**WIFIAWARE-001** are COMPLETE (**23 COMPLETE nodes**).
**🚀 OPERATOR AUTHORIZATION (DEC-0009, iter 54): ALL GATES + HUMAN GATES
UNBLOCKED.** **✅ IDENT-001 COMPLETE (iter 63c/64):** Identity System ACCEPTED
— IDENT_VERIFICATION.md v1.0 AC-1..11 all PASS, verifier APPROVE, redteam
IDENT-RT-001..016 dispositioned (PASS-with-recorded-deviations), DEC-0010
security control verified satisfied. **✅ EMERG-001 COMPLETE (iter 71):**
**VERIFY + ACCEPT** — `docs/implementation/EMERG_VERIFICATION.md` v1.0 **AC-1..
14 all PASS**; independent verifier **APPROVE** (reproduced 431 passed/0 failed,
clippy 0, `cargo audit` 0 vulns; all 6 redteam fixes present; no fake AC
evidence); redteam EMERG-RT-001..011 PASS-with-recorded-deviations (RT-001
CRITICAL authority-root requirement + RT-002 HIGH SOS rate enforcement FIXED);
3 verifier doc-level notes reconciled in-pass. **✅ SEC-001 COMPLETE (iter 82):**
**VERIFY + ACCEPT** — `docs/implementation/SEC_001_VERIFICATION.md` v1.0
**AC-1..16 all PASS**; independent verifier **APPROVE_WITH_NOTES** (reproduced
workspace 532 passed/0 failed/1 ignored, clippy 0, tarpaulin cobertura security
line-rate 0.95624, all 13 SEC-RT fixes real code with regression tests); redteam
SEC-RT 13 findings (2 CRITICAL / 8 HIGH / 2 MEDIUM + SEC-RT-15) ALL FIXED;
AC-14 95.6% (590/617 >=95% MET); AC-15 **recorded gated deviation
operator-ratified** (property-based adversarial suite + 44M+ clean fuzz execs);
AC-16 grep guard CLEAN; notes reconciled in-pass. **The four P0 SECURITY pillars
(CRYPTO-001, IDENT-001, EMERG-001, SEC-001) are all ACCEPTED.** Critical-path
junction passed.
**✅ WIFIAWARE-001 COMPLETE (iter 101) — second TRANSPORT node ACCEPTED:
AC-1..16 all PASS/GATED**; `WIFIAWARE-001_VERIFICATION.md` v1.0, verifier
**APPROVE_WITH_NOTES** (reconciled); workspace **552/0/1**, `transport::wifiaware`
11 + beacon 8, clippy **0**; SECURITY_REVIEW v2 (Pass 1 WAW-RT-001..005 + Pass 2
NEW-WA-RT-101..112 — 1 HIGH + 6 MED FIXED + 5 LOW → ANDROID-001).
**✅ BLE-001 COMPLETE (iter 90) — first TRANSPORT node ACCEPTED: AC-1..16 all
PASS (AC-14 GATED/BLK-0005 recorded)**; workspace **533/0/1**, `transport::ble`
**42**, clippy **0**, 9 RT regressions.
**🔄** Now **ANDROID-001 (P0 PLATFORM — Android app shell + Kotlin FFI, DISCOVERED → UNDERSTAND next)**:
first platform node (after WIFIDIRECT-001 ACCEPTED iter 109). Critical path:
... → EMERG-001 ✅ → **ANDROID-001** → PILOT-001. Deps BLE-001 ✅ WIFIAWARE-001 ✅
MSG-001 ✅ EMERG-001 ✅ all COMPLETE. **NODE_TRANSITION CORRECTED (iter 109b)**:
PRIORITY_POLICY P0-first selected ANDROID-001 over LORA-001 (P2,
`requires_hardware: true`) — operator ratified; LORA-001/SAT-001 deferred.
**WIFIDIRECT-001 ACCEPTED COMPLETE (iter 109)**: VERIFY (iter 108)
`WIFIDIRECT-001_VERIFICATION.md` v1.0 — AC-1..16 evidence table (AC-1..13
PASS, AC-14 GATED/BLK-0005, AC-15 SECURITY_REVIEW v1 RESOLVED, AC-16
independent verifier **APPROVE**, notes reconciled). Live re-verify: workspace
**608 passed / 0 failed / 1 ignored**; `transport::wifi_direct` **23 PASS**
(13 transport + 10 serv); clippy **0**; fmt clean. ACCEPT: graph COMPLETE,
evidence(10); PROJECT_STATE completed 23→24, implementing 1→0; CHANGELOG
0.3.39. **24 COMPLETE nodes.**
Next: **ANDROID-001 RESEARCH (iter 111, RES-0022)** → DESIGN.

**WIFIAWARE-001 carries forward (COMPLETE iter 101):** UNDERSTAND ✅ (91, C2
AC-gap) → RESEARCH ✅ (92, RES-0020 PROCEED; 8 websearch evidence passes, L1–L5)
→ DESIGN ✅ (93, WIFI_AWARE_TRANSPORT_DESIGN.md v1.0, AC-1..16 C2 RESOLVED,
DEC-WA-0001..0008, CONFLICT-1 iOS 26+ reconciliation) → IMPLEMENT ✅ (94:
wifiaware.rs `WifiAwareAdapter` FFI trait + `SimMeshCoordinator`/
`SimulatedWifiAwareAdapter` in-memory NDP mesh + `WifiAwareTransport` Transport
impl + wifiaware_beacon.rs 22-byte beacon) → TEST ✅ (95, WIFIAWARE-001_TEST.md
AC-1..16) → SECURITY_REVIEW ✅ (96 + re-review 100, v2) → VERIFY ✅ (101,
WIFIAWARE-001_VERIFICATION.md, verifier APPROVE_WITH_NOTES reconciled). v1 =
publish/subscribe NAN discovery + NDP IPv6 socket data path reusing INTERNET-001
TCP framing (1 MiB cap); runtime capability gate (FEATURE_WIFI_AWARE/
isAvailable()/state-changed, degrade BLE); FGS connectedDevice (API 34+);
app-layer envelope = trust anchor; identity by pubkey fingerprint never NAN MAC.
Background SEC-001 fuzz continues aggregating clean (49.5M+ execs) — no blocker.

## Phase Status

| Phase | Status | Notes |
|-------|--------|-------|
| WP-A: Canonical CBOR codec | ✅ COMPLETE | 61/61 tests, PROTO-001 conformance |
| WP-1: MSG-001 Message Engine | ✅ COMPLETE | 94/94 tests; MSG_VERIFICATION.md v1.1 |
| WP-2: STORE-001 Storage | ✅ COMPLETE | PostgreSQL (DEC-0002), 11/11 tests |
| WP-3: DISCO-001 Discovery | ✅ COMPLETE | 16/16 tests; DISCO_VERIFICATION.md v1.0 |
| WP-4: ROUTE-001 Routing | ✅ COMPLETE | 24/24 tests; ROUTE_VERIFICATION.md v1.0 |
| WP-5: SCF-001 Store-Carry-Forward | ✅ COMPLETE | 13 tests + M6; SCF_VERIFICATION.md v1.0 |
| WP-6: SIM-001 Simulation | ✅ COMPLETE | 13 tests; SIM_VERIFICATION.md v1.0 |
| WP-7: GW-001 Gateway Selection | ✅ COMPLETE | 19 tests (6 red-team regression); GW_VERIFICATION.md v1.1 |
| WP-8: ROUTE-002 Opportunistic Routing | ✅ COMPLETE | 17 tests (prophet 7 + opportunistic 7 + 3 sim); ROUTE2_VERIFICATION.md v1.0 |
| WP-9: OBS-001 Observability | ✅ COMPLETE | observability module + ShortId privacy + seam instrumentation; OBS_VERIFICATION.md v1.0; red-team PASS |
| WP-10: DESKTOP-001 Desktop Shell | ✅ COMPLETE | Tauri v2 shell hosting iris-core; DESKTOP_VERIFICATION.md v1.0; 225 workspace green |
| REQ-001: Requirements Baseline | ✅ COMPLETE | REQUIREMENTS_BASELINE.md 56-req traceability matrix (8 IMPLEMENTED / 17 PARTIAL / 30 DEFERRED / 2 GAP); REQ_VERIFICATION.md v1.0; verifier PASS_WITH_GAPS |
| ARCH-001: System Architecture | ✅ COMPLETE | ARCHITECTURE_BASELINE.md 7-layer + component + 20-principle reconciliation; ARCH_VERIFICATION.md v1.0; verifier PASS_WITH_GAPS |
| ML-001: ML Routing Experiments | ✅ COMPLETE | P3 EXPERIMENT, leaf; shadow-only L3 ML in sim/ml/; AC 1–7 PASS; redteam RED-0002 PASS; ML_VERIFICATION.md v1.0; verifier PASS_WITH_GAPS→corrections applied; 244 green |
| **CRYPTO-001 IMPLEMENT** | ✅ COMPLETE (iter 57) | Crypto module + codec::encode_for_aead + IrisCryptoProvider/KeyDirectory + engine encrypt/decrypt + fragment re-signing + at-rest RowSealer/schema-v2 + KAT line-up incl. RFC 5869 A.2/A.3 + sender/recipient same-key + M7 AC-8 E2E; workspace 284 green, clippy 0 |
| **CRYPTO-001 VERIFY + ACCEPT** | ✅ COMPLETE (iter 59) | CRYPTO_VERIFICATION.md v1.0 AC-1..9 all PASS; redteam RED-0001..0012 dispositioned (RED-0004 fragmentation/E2EE + 2 latent routing bugs FIXED, 290 green; RED-0001 CRITICAL operator-ratified DEC-0010 to IDENT-001); 18 COMPLETE nodes |
| **CRYPTO-001 RESEARCH** | ✅ COMPLETE (iter 55) | RES-0015 + DEC-P0001..0004: per-message ephemeral X25519 (ECIES-like), RFC 5869 HKDF-SHA256, at-rest row seal, FS honesty, crate pins + RUSTSEC audit clean |
| **🚀 All gates unblocked** | **DEC-0009 (iter 54)** | Operator authorization 2026-08-14: BLK-0001 crypto/identity, BLK-0005 transports, LEGAL-001, EMERG-001, SEC-001, TEST-001, platform apps — all eligible for implementation |
| **IDENT-001 DESIGN** | ✅ COMPLETE (iter 61) | IDENT_DESIGN.md v1.0: D1-D6 (key-derived PeerId + SHA-256 short-id; FileKeyStore + StorageKeySealer default-on; TOFU + signed ad + Verified tier; rotation/revocation null-rotation; two-keypair binding; no new crates) + AC-1..11 + wire reconciliation (32B self-auth v1, 16B collapse RED-0010/v2) + RED-0005/0008/0011 + desktop RED-0001 wiring plan |
| **IDENT-001 IMPLEMENT** | ✅ COMPLETE (iter 62) | identity module `identity/{provision,store,peer_id,advertise,trust_store,chain,rotate,small_order}` (NodeIdentityV1 provision/load + FileKeyStore + PeerId/SHA-256 short-id + RED-0005 ad + TOFU TrustStore + RED-0008 verify_chain + rotation/null-rotation + RED-0011) 58 tests + `MessageEngine::set_key_directory` + desktop RED-0001 (`DesktopIdentity`, IrisCryptoProvider, TrustKeyDirectory, persisted PeerId; DevCryptoProvider → `config.dev` test seams; IRIS_NODE_ID display-only) 5 tests; workspace green, clippy 0 |
| **IDENT-001 TEST** | ✅ COMPLETE (iter 62b) | IDENT-001_TEST.md: AC-1..9 evidence (peer_id 7, provisioning 13, advertisement 6, verify_chain 11, TrustStore 10, rotation 7, small-order 4, desktop RED-0001 3); workspace green, clippy 0 |
| **IDENT-001 SECURITY_REVIEW** | ✅ COMPLETE (iter 63) | IDENT-RT-001..016 dispositioned in IDENT-001_SECURITY_REVIEW.md — PASS-with-recorded-deviations; 9 in-scope code findings FIXED in-pass (+10 regression tests, identity 58→68), 4 gated (RT-002/003/004/005), RT-014 accepted single-instance, RT-015/016 INFO clean; DEC-0010 control VERIFIED satisfied; workspace **361 green**, clippy 0 |
| **IDENT-001 VERIFY + ACCEPT** | ✅ COMPLETE (iter 63c/64) | IDENT_VERIFICATION.md v1.0 AC-1..11 all PASS; verifier APPROVE (independently reproduced 68 identity / 361 workspace / clippy 0 / DEC-0010 by code read); IDENT-001 **COMPLETE**, 19 COMPLETE nodes |
| **IDENT-001 RESEARCH** | ✅ COMPLETE (iter 60) | RES-0016 + DEC-P0005..P0009: per-device Ed25519 identity + key-derived sender_id; per-platform provisioning (Keystore/Keychain/OS-keychain/TPM) + StorageKeySealer default-on (RED-0001); TOFU + signed advertisement + QR/pairing trust (RED-0005/0008); signed rotation/revocation; two-keypair Ed25519-signed-X25519 binding |
| **EMERG-001 TEST** | ✅ COMPLETE (iter 69) | EMERG-001_TEST.md: AC-1..12 + AC-14 test-side; CLOSED engine receive-path gap (emergency_gate in deliver_or_relay — drop-no-reply/suppress/relay + SOS bookkeeping; Noop identical AC-11); size-budget reconciled (2-elem 690 B <1 KB, 4-elem 1277 B P3); workspace 424 green, clippy 0 |
| **EMERG-001 SECURITY_REVIEW** | ✅ COMPLETE (iter 70) | EMERG-RT-001..011 dispositioned — PASS-with-recorded-deviations; FIXED: EMERG-RT-001 CRITICAL (provisioned authority root required — TOFU peer can't anchor verified chain), RT-002 HIGH (sos_rate enforced receive-side → P3 + LRU bound), RT-006 MEDIUM (bounded broadcast-replay guard), RT-007 MEDIUM (no payload bytes in audit), RT-009 LOW (poison-safe lock), RT-011 LOW (CDE duplicate-key reject); RECORDED RT-003/004/005/008/010; +7 regression tests; workspace **431 green**, clippy 0 |
| **EMERG-001 VERIFY + ACCEPT** | ✅ COMPLETE (iter 71) | EMERG_VERIFICATION.md v1.0 AC-1..14 all PASS; independent verifier APPROVE (431 passed/0 failed, clippy 0, audit 0 vulns, all 6 redteam fixes present); 3 doc-level notes reconciled (provider.rs MAX_IDS docstring 3096→1024 + ignored-test + AC-2 phrasing); **EMERG-001 COMPLETE — 20 COMPLETE nodes** |
| **SEC-001 TEST + SECURITY_REVIEW + VERIFY + ACCEPT** | ✅ COMPLETE (iter 75-82) | SEC-001_TEST.md AC-1..16; repair (77, honest correction — module did not compile); FUZZ (78, quota overflow found+fixed, 44M+ clean); redteam SEC-RT (79, 13 findings ALL FIXED); AC-14 (80, 590/617=95.6% MET + SEC-RT-15 acl decode fixed); AC-16 (81, grep guard CLEAN); AC-15 gated deviation operator-ratified + verifier APPROVE_WITH_NOTES reconciled (82). **SEC-001 COMPLETE — 21 COMPLETE nodes.** Next: transports |
| Next node | ✅ ANDROID-001 (P0 PLATFORM) DISCOVERED — UNDERSTAND NEXT | **WIFIDIRECT-001 ACCEPTED COMPLETE (iter 109)**: VERIFY (iter 108, WIFIDIRECT-001_VERIFICATION.md v1.0 AC-1..16 evidence — AC-1..13 PASS, AC-14 GATED/BLK-0005, AC-15 SECURITY_REVIEW v1 RESOLVED, AC-16 independent verifier APPROVE, notes reconciled RECORDED 6→5); live re-verify **608 passed / 0 failed / 1 ignored**, `transport::wifi_direct` **23 PASS** (13 transport + 10 serv), clippy 0, fmt clean. PRIOR SECURITY_REVIEW (iter 107, WIFIDIRECT-001_SECURITY_REVIEW.md — RT-001..013: 1 HIGH + 5 MED + 1 LOW FIXED +7 regressions, 5 RECORDED; transport::wifi_direct 23 PASS; workspace 608/0/1). PRIOR TEST (iter 106, WIFIDIRECT-001_TEST.md AC-1..13 PASS + AC-14 GATED/BLK-0005; 16 PASS; 601/0/1). PRIOR IMPLEMENT (iter 105, wifi_direct.rs + wifi_direct_serv.rs + WIFI_DIRECT_COST). PRIOR DESIGN (iter 104, WIFI_DIRECT_TRANSPORT_DESIGN.md v1.0 + AC-1..16 + DEC-WD-0001..8): v1 = Android WifiP2pManager DNS-SD + persistent GO + TCP-over-GO using INTERNET-001 framing (1 MiB cap); WPA2 floor; WPA3-SAE R2-gated; band API 29. **→ NODE_TRANSITION CORRECTED (iter 109b) → ANDROID-001 (P0 PLATFORM, UNDERSTAND iter 110) → RESEARCH (RES-0022) → DESIGN.** Next after: TEST-001 → IOS-001/PILOT-001; P2 transports LORA-001/SAT-001 deferred |

## Active Node

**ANDROID-001 — Android Platform Integration (P0 PLATFORM, DISCOVERED — iter 110
UNDERSTAND next).**
First platform node after WIFIDIRECT-001 ACCEPTED (iter 109). Implements the
Android app shell + Kotlin FFI adapters over the Rust `iris-core` engine
(BLE/WifiAware/Wi-Fi Direct transports consumed), mirroring the DESKTOP-001
Tauri v2 embedding pattern. Deps BLE-001 ✅ WIFIAWARE-001 ✅ MSG-001 ✅ EMERG-001 ✅
ALL COMPLETE. **NODE_TRANSITION CORRECTED (iter 109b)**: PRIORITY_POLICY
P0-first selected ANDROID-001 over LORA-001 (P2 `requires_hardware: true`,
deferred). Critical path → PILOT-001. Carries FFI-contract requirements
(NEW-WA-RT-108..112 + WIFIDIRECT RT-010 idempotence).
Quality process: UNDERSTAND → RESEARCH → DESIGN → IMPLEMENT → TEST →
SECURITY_REVIEW → VERIFY → ACCEPT.
**WIFIDIRECT-001 ACCEPTED COMPLETE (iter 109)** — carries forward:
- **ACCEPT (iter 109)**: graph COMPLETE + evidence(10) + stage_note full
  pipeline; PROJECT_STATE completed 23→24, implementing 1→0; CHANGELOG 0.3.39.
  **NODE_TRANSITION (iter 109b, corrected) → ANDROID-001 (UNDERSTAND, iter 110).**
- **VERIFY (iter 108) APPROVED**: `WIFIDIRECT-001_VERIFICATION.md` v1.0 AC-1..16
  evidence table; independent verifier APPROVE (all counts reproduced —
  workspace **608/0/1**, `transport::wifi_direct` **23 PASS**, clippy **0**;
  all 7 RT fixes real code + passing regressions; no nonexistent AC-cited
  tests); notes reconciled (RECORDED 6→5).
- **SECURITY_REVIEW (iter 107) RESOLVED**: WIFIDIRECT-001_RT-001..013 — 1 HIGH
  (RT-001 zero-short sentinel + empty-TXT window) + 5 MED (RT-002/003/004/005/
  007) + 1 LOW (RT-009) FIXED + 7 regression tests; 5 RECORDED.
PRIOR IMPLEMENT COMPLETE (iter 105): `wifi_direct.rs` (`WifiDirectAdapter` 20-op
FFI trait + `SimP2pCoordinator`/`SimulatedWifiDirectAdapter` deterministic
in-memory P2P mesh + `WifiDirectTransport` `Transport` impl reusing
INTERNET-001 framing, 30-s discovery re-arm, GO/GC connection table,
persistent-GO teardown) + `wifi_direct_serv.rs` (`WifiDirectTxtRecord` 22-byte
build/parse + `WIFI_DIRECT_SERVICE_NAME` + 9 adversarial tests) + `WIFI_DIRECT_COST`
+ registered in transport/mod.rs + design §7 appended.
PRIOR DESIGN COMPLETE (iter 104, WIFI_DIRECT_TRANSPORT_DESIGN.md v1.0): v1 =
Android `WifiP2pManager` DNS-SD (Bonjour) discovery on BLE-triggered re-arm
window (30-s app cadence vs 120-s framework find; `WIFI_P2P_DISCOVERY_CHANGED_ACTION` →
BLE fallback) + `createGroup`/`connect` persistent GO; wpa_supplicant dual path
(`p2p_group_add`/`p2p_connect`/`p2p_service_add bonjour`) + IRIS-owned IP/DHCP
glue (GO static + dnsmasq/udhcpd; GO address adapter-supplied, G-WD-2); **data
plane = TCP socket over GO reusing INTERNET-001 framing (1 MiB cap, pool +
backoff)** + client IPv4 DHCP / IPv6-link-local; WPA2-Personal floor (WPS-PIN
prohibited, passphrase over authenticated BLE, PBC legacy fallback only);
WPA3-SAE R2 capability-gated; `setGroupOperatingBand` API 29 5 GHz preferred /
AUTO fallback; coex `WifiAvailableChannel` API 34; N-client admission + GO-side
connection table (G-WD-1); OS patch floor AC-14; FGS connectedDevice + wakelock
background contract. **AC-1..16 defined (C2 gap RESOLVED)** + **DEC-WD-0001..0008
ratified** + WIFI_DIRECT.md corrections (band API 29, find window 120 s, client
ceiling vendor/HAL, WPA3-SAE R2). BLK-0005 lifted by DEC-0009 for
implementation; **device-only integration tests remain RESOURCE-gated** (physical
hardware) — recorded known_limitations; Android Kotlin adapter = ANDROID-001
scope (carries NEW-WA-RT-108..112 FFI requirements from WIFIAWARE-001).
Reference patterns: `INTERNET_TRANSPORT_VERIFICATION.md` +
`crates/iris-core/src/transport/` + BLE-001 (iters 83–90) + WIFIAWARE-001
(iters 91–101) pipelines.

**WIFIAWARE-001 COMPLETE (iter 101) — carries forward (second TRANSPORT):**
- **ACCEPT (iter 101)**: `WIFIAWARE-001_VERIFICATION.md` v1.0 AC-1..16 evidence;
  verifier **APPROVE_WITH_NOTES** (reconciled — display string fix); workspace
  **552/0/1**, `transport::wifiaware` 11 + beacon 8 = 19 under shared filter,
  clippy **0**; SECURITY_REVIEW v2 (Pass 1 WAW-RT-001..005 + Pass 2
  NEW-WA-RT-101..112: 1 HIGH recv-side zero-peer attribution FIXED + 6 MED FIXED
  + 5 LOW → ANDROID-001). status COMPLETE, evidence(11).
- **VERIFY (iter 101)**: `WIFIAWARE-001_VERIFICATION.md` AC-1..16; independent
  verifier reproduced all counts, no AC row cites a nonexistent test.

**BLE-001 COMPLETE (iter 90) — carries forward (first TRANSPORT):**
- **ACCEPT (iter 90)**: graph BLE-001 → COMPLETE, evidence 7, known_limitations
  14, AC-1..16; workspace **533/0/1**, `transport::ble` **42**, clippy **0**.
- **VERIFY (iter 89) APPROVED**: `docs/implementation/BLE_001_VERIFICATION.md`
  v1.0 AC-1..16 evidence; independent verifier APPROVED (all 9 RT fixes real
  code + real regressions, 22 AC-cited tests exist, AC-14 honest).
- **SECURITY_REVIEW (iter 88) RESOLVED**: BLE-RT-001..016 (RT-008 absent); 7
  HIGH/MED FIXED + RT-009 RECORDED + 3 LOW FIXED + 5 RECORDED; 9 RT regressions.

**RESEARCH COMPLETE (iter 73) — RES-0018 recorded + registered (next
RES-0019).** Decisions at a glance (R1–R12): ADOPT srTCM token bucket per
(sender,class) silent-drop P0/P1-exempt + per-sender storage quota +
priority-reserved pool + TTL-ordered eviction (RFC 2697/2698; RFC 9171 §6.9;
Claim-Carry-and-Check; Meshtastic CVE regression suite); **REJECT** per-message
PoW (battery/duty-cycle/difficulty-calibration) — **DEFER** identity-mint PoW
(KeyChallenge/SyDeLP) + VDF + RFC 7859 to PROTO-001 v2 (no new crypto crates);
**ADOPT** layered replay = dedup + freshness window τ (per-source skew) +
per-sender high-water (ts,seq) + **promote Bloom+LRU persistence WP-2→SEC-001**
(cross-reboot replay closure, `mod.rs:24`); **ADOPT** Ostra link-credit + Briar
BQP verified-pairing Sybil layer (REJECT SybilGuard/SybilLimit on relays);
**ADOPT** reputation = local watchdog + positive-only verified second-hand
(CORE) + iTrust audits as **routing weight only** (Watchdog/Pathrater, iTrust,
ITRM, Rep-AODV 2024); **ADOPT** RED-0002 ACL-1 key-anchored per-class sender
allowlist (over `emergency/authority.rs`); **ADOPT** receiver-side Bayesian
spam scoring only; **REJECT** RTT/GPS wormhole/secure-position; DEFER SAND.
CONFLICTS C1–C6 (= DISC-0013 doc-vs-code drift) feed C-pattern doc
reconciliation. Gaps G1–G7 (τ → EXP-SEC-001). **Next: DESIGN**
(SEC_001_DESIGN.md + ACs + DEC-SEC-0001..N).

**IMPLEMENT COMPLETE (iter 74) — SEC_001_DESIGN.md v1.0 fully implemented.**
Built `security/` module: rate_limiter.rs (srTCM RFC 2697), quota.rs (per-sender
quota + priority pool), replay.rs (freshness + high-water + cross-reboot
persistence), reputation.rs (Bayesian routing weight), spam.rs (receiver-side
likely_spam), acl.rs (ACL-1 emergency auth), mod.rs (SecurityPolicy + Noop
facade). **Integrated into MessageEngine:** outbound rate_limit + quota check,
inbound replay protection + spam scoring + emergency ACL. **(⚠️ The iter-74 "464
tests green, clippy 0" claim was FALSE — corrected iter 77 to an honest 497/499
baseline.)**

**🔄 TEST IN PROGRESS — repair (77) + redteam SEC-RT (79) + AC-14 (80) COMPLETE.** Repair
pass: async `SecurityPolicy` + `FullSecurityPolicy` real-engine routing + all
6 proptest modules rewritten (`block_on`), 10 clippy warnings fixed, 7 proptest
failures triaged (6 test bugs + 1 real ACL bug). Redteam SEC-RT (AC-16):
**12 findings (2 CRITICAL / 8 HIGH / 2 MEDIUM) all fixed** — replay deadlock
(SEC-RT-01), ACL real chain verification (SEC-RT-02), replay future-poison
clamp + regression (SEC-RT-03/14), quota TOCTOU single gate + terminal refunds
(SEC-RT-04/05), spam cap (SEC-RT-06), reputation verified-secondhand gate
(SEC-RT-07), SOS empty-store gate (SEC-RT-08), unknown-sender burst (SEC-RT-09),
div-by-zero (SEC-RT-11), saturating eviction (SEC-RT-12). Fuzz (AC-15): harness
proven, 19.4 M exec clean, quota overflow found+fixed (iter 78). **AC-14 (iter 80):
tarpaulin re-measured honestly — security module 590/617 = 95.6% (>=95% target
MET)** via 40+ targeted coverage tests; discovered + fixed **SEC-RT-15** (acl
root decode unreachable authorized path). **Verified: workspace 499 green / 0
failed / 1 ignored (iter 79), iris-core 444, security 68 passed, clippy 0.**
Remaining: fuzz >=24h aggregate, AC-16 remainder (SEC-RT-15 record row + grep
guard + known-limitations) → SECURITY_REVIEW → VERIFY → ACCEPT.

## EMERG-001 COMPLETE (iter 71 — carries forward)

VERIFIED + ACCEPTED: EMERG_VERIFICATION.md v1.0 **AC-1..14 all PASS**;
independent verifier **APPROVE** (reproduced 431 passed/0 failed, clippy 0,
`cargo audit` 0 vulns across 516 crates, all 6 redteam fixes present with
regression tests, no fake AC evidence). Redteam EMERG-RT-001..011
PASS-with-recorded-deviations (RT-001 CRITICAL provisioned-authority-root
requirement + RT-002 HIGH SOS receive-side rate enforcement + RT-006 replay + 
RT-007 audit privacy + RT-009/RT-011 LOW all FIXED in-pass; RT-003/004/005/008/
010 recorded). emergency/ module + engine receive-path gate + Noop default
(AC-11). known_limitations: BLE 512-B verified-chain ceiling (SOS radio-
native), send-side synthesis app-owned, typed EmergencyEvent platform deferral,
RT-003/004/005/008/010, 17 pre-existing desktop-transitive audit warnings.
**IMPLEMENT (68) → TEST (69) → SECURITY_REVIEW (70) → VERIFY+ACCEPT (71)**.

## IDENT-001 COMPLETE (iter 63c/64) — carries forward

IDENT-001 ACCEPTED: IDENT_VERIFICATION.md v1.0 AC-1..11 all PASS (verifier
APPROVE); redteam IDENT-RT-001..016 PASS-with-recorded-deviations (9 in-scope
code findings FIXED +10 regression tests, identity 68); DEC-0010 control
verified satisfied (production path = IrisCryptoProvider + TrustKeyDirectory +
persisted DesktopIdentity; DevCryptoProvider only `config.dev`). identity
module `identity/{provision,store,peer_id,advertise,trust_store,chain,rotate,
small_order}` + desktop RED-0001. known_limitations: RT-002/003/004/005 gated,
RT-014 accepted. Provides EMERG-001 inputs: peer_id/peer_short (emergency
sender identity), trust/rotation.

### CRYPTO-001 COMPLETE (iter 59) — carries forward

VERIFIED + ACCEPTED: CRYPTO_VERIFICATION.md v1.0 AC-1..9 all PASS. Redteam
RED-0001..0012 dispositioned: RED-0004 fragmentation/E2EE + 2 latent routing
bugs FIXED (M7 fragment E2E PASS, 290 green), RED-0002 mitigated (warn +
metric + threat model), RED-0006/0007/0009/0012 fixed; RED-0001 CRITICAL
operator-ratified (DEC-0010) → IDENT-001; RED-0003/0005/0008/0010/0011 tracked.
Design (iter 56, CRYPTO_DESIGN.md: wire/AAD/KDF/at-rest contracts,
IrisCryptoProvider + KeyDirectory, P0 broadcast non-encryption, KAT line-up,
crate pins) fully implemented (iter 57): crypto module, `codec::encode_for_aead`
(fields 1–7, 9–11, 14), engine send-path encrypt (recipient X25519 via
KeyDirectory; **P0 broadcast never encrypted**) + deliver-path decrypt/verify,
at-rest `RowSealer`/`StorageKeySealer` (AAD = message_id‖priority‖expires_at,
per-row random 12B nonce) + `NoSealer` default + schema v2 plaintext-identity
columns dropped, KATs RFC 7748/8439/8032/5869, M7 E2E suite (round-trip,
tampered-frame, fragmented re-verify).

NEXT: **EMERG-001 COMPLETE (iter 71)** — EMERG_VERIFICATION.md v1.0 AC-1..14
PASS, verifier APPROVE, 431 green. **🔄 SEC-001 active**: UNDERSTAND →
RESEARCH → DESIGN → IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT.
Then transports (BLE-001/WIFIAWARE-001/WIFIDIRECT-001/LORA-001/
SAT-001) → TEST-001 → platform apps.

## Recent Milestones

- 2026-08-17: **WIFIDIRECT-001 ACCEPTED COMPLETE (iter 108 VERIFY + iter 109 ACCEPT) — third TRANSPORT node done, 24 COMPLETE nodes.** VERIFY: `engineering/memory/records/WIFIDIRECT-001_VERIFICATION.md` v1.0 AC-1..16 evidence table (pattern WIFIAWARE-001_VERIFICATION.md): AC-1..13 PASS, AC-14 GATED/BLK-0005, AC-15 SECURITY_REVIEW v1 RESOLVED (RT-001..013), AC-16 independent verifier reproduction. Live re-verify iter 108: workspace **608 passed / 0 failed / 1 ignored** (per-suite sum reconciled: iris-core lib 553 + crypto_e2e 3, ml 8, obs 4, sim 8+1, desktop 5, commands 3, engine_roundtrip 3, storage 7, m3 1, pg_store 13); `transport::wifi_direct` **23 PASS** (13 transport + 10 serv under shared prefix); clippy --workspace --all-features --tests **0**; fmt clean. **Independent verifier APPROVE** (ses_ff2cab279ffebsAWkoU2shmMk0): all counts reproduced; all 7 RT fixes real code with file:line refs + passing regressions (RT-001 `ZeroShortId` gate + atomic `upsert_peer`; RT-002 same-dest eviction; RT-003/012 real-GO-only join; RT-004 reuse re-promotes; RT-005 links→Degraded; RT-007 empty-payload reject; RT-009 doc); no AC row cites a nonexistent test. Notes reconciled in-pass: RECORDED count corrected **6→5** (RT-009 doc-FIXED counted in the 7; security-review header + section label + verification AC-15 row updated); clippy-cache + empty-suite nits non-blocking. ACCEPT: PROJECT_GRAPH WIFIDIRECT-001 status → **COMPLETE**, evidence(10), stage_note full pipeline (UNDERSTAND 102 → RESEARCH 103 → DESIGN 104 → IMPLEMENT 105 → TEST 106 → SECURITY_REVIEW 107 → VERIFY 108 → ACCEPT 109), known_limitations(12); PROJECT_STATE completed 23→24, implementing 1→0; CHANGELOG 0.3.39. **NODE_TRANSITION → LORA-001 (UNDERSTAND, iter 110).** Transport pipeline: INTERNET-001 ✅ → BLE-001 ✅ → WIFIAWARE-001 ✅ → WIFIDIRECT-001 ✅ → LORA-001 → SAT-001.

- 2026-08-17: **WIFIDIRECT-001 SECURITY_REVIEW COMPLETE (iter 107) — WIFIDIRECT-001_RT-001..013 dispositioned (1 HIGH + 5 MEDIUM + 1 LOW FIXED in-pass + 7 regression tests; 6 RECORDED), third transport staged for VERIFY.** `WIFIDIRECT-001_SECURITY_REVIEW.md` (AC-15, pattern BLE-001/WIFIAWARE-001): adversarial review of `wifi_direct.rs` / `wifi_direct_serv.rs` / transport wiring + `WIFI_DIRECT_COST`. **FIXED**: **RT-001 (HIGH)** all-zero `peer_short` TXT mapped to the "unknown-sender" sentinel `PeerId([0u8;32])` in `candidate_peer_id_pads_short_id` + sim `register()` empty-TXT window → `parse` now rejects `ZeroShortId` (`ZeroShortId` error variant) + `coordinator.upsert_peer` atomically registers tag+TXT under one lock (`advertised_txt_is_attributable`); **RT-002** outbox global-oldest eviction starved other destinations → per-destination eviction (`outbox_eviction_keeps_other_destinations`); **RT-003/RT-012** phantom-group manufacture for non-GO peers + `group_id` overwrite → join requires existing GO + single-group-per-adapter guard (`join_non_go_peer_is_rejected`); **RT-004** connect reuse didn't re-promote Connected → `set_state(Connected)` on reuse (`connect_reuse_after_churn_recovers`); **RT-005** send-on-unavailable left link Connected → tears all links down → Degraded (`send_on_unavailable_tears_down_links`); **RT-007** empty zero-length payload acked but dropped inbound → Protocol reject on send (`empty_payload_send_rejected`); **RT-009** INCOMING_CHANNEL_CAPACITY doc comment 8 MiB → 32 MiB corrected. **RECORDED (6)**: RT-006 (dropped_inbound lag telemetry undercounts broadcast), RT-010 (ensure_started latch prevents FFI double-start → ANDROID-001 idempotence requirement), RT-011 (RadioConflictGroup → TRANSPORT-001 manager arbitration), RT-013 (shared 1 MiB constant refactor), poll cadence (100 Hz) battery tradeoff, AC-14 GATED/BLK-0005. *(Count later corrected to 5 at VERIFY — RT-009 is doc-FIXED, counted in the 7.)* Positive controls held: strict TXT parse, framed decode bounds, candidate-only trust boundary, gated write path, bounded queues, lifecycle/shutdown latch, group-model integrity, log hygiene. **Live re-verify: `transport::wifi_direct` 23 PASS (13 transport + 10 serv incl. 7 RT regressions); workspace --all-features 608 passed / 0 failed / 1 ignored (baseline 601, +7 RT); clippy --workspace --all-features --tests 0; fmt clean.** PROJECT_GRAPH evidence(9) + known_limitations(12) + stage_note SECURITY_REVIEW COMPLETE; status held IMPLEMENTING (moves at VERIFY→ACCEPT). **STAGE_TRANSITION → VERIFY (iter 108).**

- 2026-08-17: **WIFIDIRECT-001 TEST COMPLETE (iter 106) — WIFIDIRECT-001_TEST.md AC-1..16 evidence table (AC-1..13 PASS + AC-14 GATED/BLK-0005), third transport staged for SECURITY_REVIEW.** Evidence mapping + live re-verify: AC-1 `registration_gating` (TransportManager never selects Unavailable), AC-2 `discovery_finds_advertising_peer` + serv `build_parse_round_trip`/`service_name_is_fixed`/`candidate_peer_id_pads_short_id` (DNS-SD 'com.iris.mesh.v1' + 22-byte TXT-record → candidate PeerId), AC-3 `band_restricted_go_creation_falls_back` (AUTO fallback DEC-WD-0005 + unavailable connect gate), AC-4 `group_roundtrip_delivers_payload` (GO go_intent 14 + GC 0 → INTERNET-001-framed payload E2E → engine seam; sender NEVER zero + attributed to real sender), AC-5 `discovery_rearm_and_stop` (30-s DISCOVERY_WINDOW + stop clears window = WIFI_P2P_DISCOVERY_CHANGED_ACTION → BLE fallback), AC-6 9 serv adversarial tests (exhaustive 0..22 short-len; unsupported version; unknown kind; reserved bits; oversize padded ≤255 cap; past-cap TooLong + boundary ok), AC-7 candidate-only pubkey identity never P2P MAC (DEC-WD-0007), AC-8 `shutdown_returns_to_unavailable` + persistent-GO teardown, AC-9 `single_link_per_peer_and_bounded_table` (8 concurrent → 1 link; MAX_GO_CLIENTS=8), AC-10 WIFI_DIRECT.md FGS/wakelock + band API 29 + 120-s find/30-s re-arm docs + caps background false, AC-11 20-op FFI conformance clippy 0, AC-12 doc reconciliation, AC-13 workspace 601/0/1 + clippy 0. **16 tests PASS live (7 transport + 9 serv); workspace --all-features 601 passed / 0 failed / 1 ignored; clippy 0; fmt clean.** PROJECT_GRAPH evidence(8) + stage_note/validation_status iter 106; status held IMPLEMENTING (moves at VERIFY→ACCEPT). **STAGE_TRANSITION → SECURITY_REVIEW (iter 107).**

- 2026-08-17: **WIFIDIRECT-001 IMPLEMENT COMPLETE (iter 105) — wifi_direct.rs + wifi_direct_serv.rs + WIFI_DIRECT_COST registered, third transport staged for TEST.** `wifi_direct.rs`: `WifiDirectAdapter` FFI trait (20 ops) + `SimP2pCoordinator`/`SimulatedWifiDirectAdapter` in-memory P2P mesh (per-tag DNS-SD service-name match, GO/GC group formation, bounded outbox MAX_OUTBOX_FRAMES=128 + per-call drain budget, availability churn, band-restriction failure) + `WifiDirectTransport` `Transport` impl (capability/coexistence gate AC-3, DNS-SD discovery → candidates AC-2, 30-s app re-arm window AC-5, GO/GC connection table + INTERNET-001 framing reuse AC-4/AC-9, lifecycle + persistent-GO teardown AC-8, envelope seam AC-7). `wifi_direct_serv.rs` (AC-6): `WifiDirectCapBits` + `WifiDirectTxtRecord` 22-byte build/parse + `WIFI_DIRECT_SERVICE_NAME` "com.iris.mesh.v1" + 9 adversarial tests. `WIFI_DIRECT_COST` BatteryCostModel added (scan 80 / advertise 50 / connected 40 / tx 0.01 / rx 0.008 — design estimates per AC-14). Both modules registered in transport/mod.rs; design doc §7 reference-impl section appended (AC→code mapping + test suite + interop note). **16 tests PASS (7 transport + 9 serv); workspace 601 passed / 0 failed / 1 ignored; clippy 0; fmt clean.** Authoring fixes in-pass (group_info Vec<u64>→PeerHandle, ConnectionFailed unit variant, MutexGuard deref, OperatingBand derived Default, join_all→sequential). PROJECT_GRAPH status DESIGNING→IMPLEMENTING + evidence(7) + stage_note. PROJECT_STATE designing 1→0, implementing 0→1. **STAGE_TRANSITION → TEST (iter 106).**

- 2026-08-17: **WIFIDIRECT-001 DESIGN COMPLETE (iter 104) — WIFI_DIRECT_TRANSPORT_DESIGN.md v1.0 + AC-1..16 C2 RESOLVED + DEC-WD-0001..8 ratified, third transport staged for IMPLEMENT.** Design per RES-0021 verdict (Q1–Q8 + G-WD-1..8 + D-1..D-7): v1 = Android `WifiP2pManager` DNS-SD (Bonjour) discovery on BLE-triggered re-arm window (30-s app cadence vs 120-s framework find) + `createGroup`/`connect` persistent GO; wpa_supplicant dual path (`p2p_group_add`/`p2p_connect`/`p2p_service_add bonjour`) + IRIS-owned IP/DHCP glue (GO address adapter-supplied, never hard-coded, G-WD-2); **data plane = TCP socket over GO reusing INTERNET-001 framing (1 MiB cap, pool+backoff)** + client IPv4 DHCP / IPv6-link-local; WPA2-Personal floor (WPS-PIN prohibited, passphrase over authenticated BLE, PBC legacy fallback only); WPA3-SAE R2 capability-gated; `setGroupOperatingBand` API 29 5 GHz preferred / AUTO fallback; coex `WifiAvailableChannel` API 34; N-client admission + GO-side connection table (G-WD-1); OS patch floor AC-14; FGS connectedDevice + wakelock contract. PROJECT_GRAPH: WIFIDIRECT-001 DISCOVERED→DESIGNING + **acceptance_criteria AC-1..16** (C2 gap RESOLVED) + evidence(6) + known_limitations(8). DEC-WD-0001..0008 ratified. WIFI_DIRECT.md corrections (band API 29, find window 120 s framework / 30 s app re-arm, client ceiling vendor/HAL + N-client, WPA3-SAE R2 + WPS-PIN prohibited, OS patch floor block). Baseline **552/0/1 clippy 0** held (docs-only pass). **STAGE_TRANSITION → IMPLEMENT (iter 105).**

- 2026-08-17: **WIFIDIRECT-001 RESEARCH COMPLETE (iter 103) — RES-0021 PROCEED,
  third transport staged for DESIGN.** RES-0021 recorded + registered
  (ALLOCATION next RES-0022): 8 websearch evidence passes (2024–2026 Wi-Fi
  Direct/P2P SOTA, primary sources, L1–L5, no AI citations). Key findings:
  WifiP2pManager alive + extended API 36/37 (no deprecation; DPP infra-only);
  **WPA2-Personal floor + WPA3-SAE R2-capability-gated**; **"1 GO + 8 clients" =
  vendor/HAL ceiling** (G-WD-1) → N-client admission; DNS-SD discovery preferred
  with **framework find window 120 s** (`DISCOVER_TIMEOUT_S`; doc 30-s = app
  re-arm, G-WD-7); persistent GO + **`setGroupOperatingBand` = API 29** (doc
  correction); WPS deprecated API 28 + **WPS-PIN prohibited** (CVE-2021-0326;
  passphrase pushed over authenticated BLE, PBC legacy fallback); wpa_supplicant
  full P2P surface + **IRIS-owned IP/DHCP glue** (G-WD-2); **OS patch floor AC**
  (SPL ≥ 2021-02, wpa_supplicant ≥ 2.12, kernel w/ 2024–26 Wi-Fi fixes);
  gaps G-WD-1..8 + DESIGN decisions D-1..D-7. Baseline **552/0/1 clippy 0** held
  (research-only). **STAGE_TRANSITION → DESIGN (iter 104).**

- 2026-08-17: **WIFIDIRECT-001 UNDERSTAND COMPLETE (iter 102) — C2 AC-gap
  found, third transport staged for RESEARCH.** Read PROJECT_GRAPH node (P1
  TRANSPORT, DISCOVERED, deps TRANSPORT-001 COMPLETE, platform ANDROID).
  WIFI_DIRECT.md (326 lines) verified: WD = data plane triggered by BLE control
  plane (4-phase workflow), GO/client topology (GO IP 192.168.49.1), up to
  200 m / 250 Mbps peak / 10-80 Mbps practical / setup 2-15 s, DNS-SD service
  discovery preferred, foreground/FGS + wakelock required (BLE handles
  background discovery; WD active transfers only), 1 GO + up to 8 clients, GO
  intent bias (14/7/3), 30-s discovery window, persistent GO optimization,
  same-band infra-Wi-Fi interference (setGroupOperatingBand API 29), iOS NOT
  available → MCSession Apple-only (known gap). TRANSPORT_ABSTRACTION row
  (50-200 m / ~250 Mbps / 10-80 Mbps / 50-500 ms setup / High battery /
  P0-P3 files). manager.rs:460 stub `wifi-direct` already registered in test;
  no production `wifi_direct.rs`. Baseline **552/0/1 clippy 0** held (read-only
  pass); PROJECT_GRAPH evidence(4) + known_limitations(5) + stage_note.
  **STAGE_TRANSITION → RESEARCH (iter 103, RES-0021).**

- 2026-08-16: **WIFIAWARE-001 ACCEPTED COMPLETE (iter 101) — second TRANSPORT
  node done, 23 COMPLETE nodes.** VERIFY: `WIFIAWARE-001_VERIFICATION.md` v1.0
  AC-1..16 evidence table (AC-1..13 PASS, AC-14 GATED/BLK-0005, AC-15
  SECURITY_REVIEW v2 RESOLVED, AC-16 independent verifier); live re-verify
  workspace **552/0/1**, `transport::wifiaware` 11 + beacon 8 = 19 under shared
  filter, clippy **0**; independent verifier **APPROVE_WITH_NOTES** (all counts
  reproduced, all 19 tests present, both HIGH SECURITY_REVIEW fixes real code);
  note reconciled — display string `"Wi-Fi Aware (scaffold)"` → `"Wi-Fi Aware"`.
  ACCEPT: graph COMPLETE + evidence(11) + NEW-WA-RT-108..112 carry-forward;
  PROJECT_STATE completed 22→23; CHANGELOG 0.3.33. **NODE_TRANSITION → WIFIDIRECT-001
  (UNDERSTAND iter 102).**

- 2026-08-16: **WIFIAWARE-001 SECURITY_REVIEW COMPLETE (iter 96 + re-review
  iter 100) — CONDITIONAL-FAIL RESOLVED.** `WIFIAWARE-001_SECURITY_REVIEW.md`
  v2 (AC-15): **Pass 1** redteam WAW-RT-001..005 (1 HIGH concurrent-connect
  double-open AC-9, 1 MED sim unbounded outbox, 3 LOW) ALL FIXED with
  regressions. **Pass 2** independent red-team re-review of data paths +
  concurrency found **NEW-WA-RT-101..112**: **HIGH NEW-WA-RT-101** — recv-side
  attribution ALWAYS `PeerId([0u8;32])` (sim forwarded the SENDER's opaque NDP
  handle, poller matched the RECEIVER's own links) → fixed via
  `IncomingNdpData.sender: Option<PeerId>` + sim outbox `(to_tag, from_tag)` +
  beacon-derived candidate attribution; AC-4 test now asserts real sender
  PeerId. **6 MEDIUM ALL FIXED**: NEW-WA-RT-102 connect `is_available` gate
  (`NotSupported`), 103 availability re-check after `open_ndp.await`,
  104 `teardown_link`→Degraded when scope down, 105 per-tick budget enforced in
  the transport (poller backlog, no drop), 106 `dropped_inbound` telemetry,
  107 `is_link_loss_error` transient-vs-terminal classifier. **5 LOW RECORDED**
  as ANDROID-001 FFI-contract requirements (verified-peer reuse key, closed-NDP
  frame prune, FFI timeouts, adapter idempotence, ring-buffer outbox). New
  regressions: `transient_send_error_keeps_link`,
  `last_link_death_while_unavailable_is_degraded`,
  `connect_while_unavailable_is_not_supported` + strengthened
  `ndp_roundtrip_delivers_payload`. **Workspace 552/0/1 (wifiaware 19 + beacon
  8), clippy 0.** STAGE_TRANSITION to VERIFY (AC-16, iter 101).

- 2026-08-16: **WIFIAWARE-001 TEST COMPLETE (iter 95)** —
  `engineering/memory/records/WIFIAWARE-001_TEST.md` AC-1..16 evidence (pattern
  BLE-001_TEST.md): AC-1 registration gating, AC-2 discovery candidate flow,
  AC-3/AC-5 availability gate + churn, AC-4 NDP E2E round-trip, AC-6 7 adversarial
  beacon tests, AC-7 candidate-only + pubkey identity, AC-8 lifecycle, AC-9
  per-peer single NDP, AC-10 background/FGS doc, AC-11 FFI conformance, AC-12 doc
  reconciliation, AC-13 workspace green + clippy 0, AC-14 GATED/BLK-0005, AC-15
  SECURITY_REVIEW next, AC-16 VERIFY. **transport::wifiaware 12 PASS, beacon 7
  PASS, clippy 0, workspace green (490 lib).** Durable-state drift reconciled
  (execution-state/ACTIVE_NODE/NEXT_ACTION were stale at iter 93 → all at iter 95).
  Next: SECURITY_REVIEW (iter 96).

- 2026-08-16: **WIFIAWARE-001 IMPLEMENT COMPLETE (iter 94)** —
  `crates/iris-core/src/transport/wifiaware.rs` (`WifiAwareAdapter` trait = FFI
  contract + `SimulatedWifiAwareAdapter`/`SimMeshCoordinator` deterministic
  in-memory NDP mesh + `WifiAwareTransport` Transport impl reusing
  INTERNET-001 framing, NDP poller, state machine) + `wifiaware_beacon.rs`
  (22-byte beacon build/parse + `CapabilityBits` + `candidate_peer_id` +
  adversarial tests) + registered in transport/mod.rs + design §9 ref-impl
  section. **transport::wifiaware 12 PASS, beacon 7 PASS, clippy 0, fmt clean,
  workspace green (490 lib).** Status DESIGNING→IMPLEMENTING; PROJECT_STATE
  implementing 0→1. Next: TEST (iter 95).

- 2026-08-16: **WIFIAWARE-001 DESIGN COMPLETE (iter 93)** —
  `docs/implementation/WIFI_AWARE_TRANSPORT_DESIGN.md` v1.0 written per
  RES-0020 verdict: v1 = publish/subscribe NAN discovery + NDP IPv6 socket data
  path reusing INTERNET-001 TCP framing (1 MiB cap); runtime capability gate;
  FGS connectedDevice (API 34+) required for production discovery; NDP open at
  L2, app-layer envelope = trust anchor; identity by pubkey fingerprint never
  NAN MAC. **AC-1..16 defined (C2 gap RESOLVED)**; status DISCOVERED→DESIGNING;
  **DEC-WA-0001..0008 ratified**; **CONFLICT-1 reconciled** (Apple WiFiAware
  iOS 26+ — "iOS NO" rows corrected in WIFI_AWARE.md + TRANSPORT_ABSTRACTION).
  PROJECT_STATE designing 0→1, discovered 9→8. Next: IMPLEMENT (iter 94).

- 2026-08-16: **WIFIAWARE-001 RESEARCH COMPLETE (iter 92)** — RES-0020 recorded
  (8 websearch evidence passes, primary sources L1–L5, no AI citations),
  verdict **PROCEED**: v1 = publish/subscribe NAN discovery + NDP IPv6 socket
  data path reusing INTERNET-001 TCP framing (1 MiB cap); NAN alive API 26→36
  (not deprecated); FGS required for production discovery; app-layer envelope =
  trust anchor; **CONFLICT-1: Apple WiFiAware framework (iOS 26+)** → 'iOS NO'
  rows corrected at DESIGN; gaps G-WA-1/2/3. ALLOCATION next RES-0021;
  CHANGELOG 0.3.31; graph evidence(4) + known_limitations(7). Status held
  DISCOVERED. Next: DESIGN (iter 93, AC-1..n).

- 2026-08-16: **BLE-001 ACCEPTED (iter 90) — first TRANSPORT node COMPLETE.**
  AC-1..16 all PASS (AC-14 GATED/BLK-0005 recorded); graph COMPLETE + evidence 7
  + known_limitations 14; PROJECT_STATE completed 21→22, verifying 1→0;
  CHANGELOG 0.3.27/28/29; NODE_TRANSITION → WIFIAWARE-001 (UNDERSTAND iter 91).
- 2026-08-16: **BLE-001 VERIFY COMPLETE (iter 89) — AC-16 APPROVED.** Wrote
  `docs/implementation/BLE_001_VERIFICATION.md` v1.0: AC-1..16 evidence table
  (AC-14 GATED/BLK-0005, AC-15 RESOLVED, AC-16 independent reproduction). Live
  re-verify: workspace **533/0/1**, `transport::ble` **42**, clippy **0**, 9 RT
  regressions pass by name. Independent verifier subagent **APPROVED** — all RT
  fixes real code with real regression tests (TTL sweep at ble.rs poller, connect
  full-setup lock + prior AbortHandle abort, per-connection MTU map +
  `segment_for_mtu`, partition-in-place drain), all 22 grepped AC-cited tests
  exist, AC-14 honest. Notes reconciled: RT-009 aggregate counts (RECORDED not
  FIXED), AC-10 §3 ref, "taref" typo. **BLE-001 → ACCEPT (iter 90).**
- 2026-08-16: **BLE-001 SECURITY_REVIEW COMPLETE (iter 88) — CONDITIONAL-FAIL
  RESOLVED.** Redteam BLE-RT-001..016 dispositioned in
  BLE-001_SECURITY_REVIEW.md (AC-15): 4 HIGH + 4 MEDIUM + 5 LOW + 2 INFO; 8
  HIGH/MED FIXED + 3 LOW FIXED + 5 RECORDED. HIGH: RT-001 (reassembly TTL
  eviction dead code → poller 1-s evict_stale sweep; partial-slot exhaustion was
  a permanent multi-chunk DoS), RT-002 (connect TOCTOU → full-setup connections
  lock + abort prior poller), RT-003 (global MTU → per-connection
  `HashMap<PeerId,(GattHandle,u16)>` + `segment_for_mtu`), RT-016
  (partition-in-place by own GattHandle — no cross-peer frame misattribution).
  MED: RT-004 (connect failure → Available), RT-005 (write-failure `close_peer`
  teardown). LOW: RT-007 (encode_frame → Result), RT-011 (readvertise stops
  prior handle), RT-013 (max_peers). 9 RT regression tests + adapters
  (TwoMtu/FailingWrite/FailingConnect). Independent verifier **APPROVE_WITH_
  NOTES** (F1: session count 533; F2: inventory 42 — applied). **Workspace
  533/0/1, transport::ble 42, clippy 0.** STAGE_TRANSITION to VERIFY (AC-16,
  iter 89).
- 2026-08-16: **BLE-001 TEST COMPLETE (iter 87)** — BLE-001_TEST.md AC-1..13
  software-evidence + AC-10 background/FGS docs + AC-12 doc reconciliation;
  baseline 524/0/1 workspace. STAGE_TRANSITION to SECURITY_REVIEW.
- 2026-08-15: **SEC-001 RESEARCH COMPLETE (iter 73) — STAGE_TRANSITION to
  DESIGN.** RES-0018 recorded (255 lines) + registered in ALLOCATION.md (next
  RES-0019): 2024–2026 SOTA for DTN/mesh DoS + replay + Sybil + spam + routing
  attacks (12 websearch evidence passes, evidence-leveled RFC/NIST L1 /
  academic L2 / IETF-draft L3 / OSS L4; no AI citations). **R1–R12 decisions**:
  ADOPT srTCM token bucket per (sender,class) silent-drop P0/P1-exempt (RFC
  2697/2698; RFC 9171 §6.9) + per-sender storage quota/priority pool +
  TTL-ordered eviction + Meshtastic CVE regression suite (CVE-2024-47065
  traceroute amplification, want_response crash, parser, amplification, key
  hygiene); REJECT per-message PoW (battery/duty-cycle/difficulty-
  calibration), DEFER identity-mint PoW (KeyChallenge/SyDeLP) + VDF + RFC 7859
  to PROTO-001 v2 (no-new-crypto held); ADOPT layered replay = dedup +
  freshness window τ + per-sender high-water (ts,seq) (RFC 9171 §4.2.7, RFC
  4303 §3.4.3, RFC 7181 §23.2/RFC 7183, RFC 6479) + **Bloom+LRU persistence
  promoted WP-2→SEC-001** (cross-reboot closure, `mod.rs:24`); ADOPT Ostra
  link-credit + Briar BQP verified-pairing Sybil layer, REJECT
  SybilGuard/SybilLimit on relays; ADOPT reputation as routing weight only
  (CORE-positive second-hand, iTrust audits); ADOPT RED-0002 ACL-1 key-anchored
  per-class sender allowlist (over `emergency/authority.rs`); ADOPT
  receiver-side Bayesian spam scoring; ADOPT blackhole = replication +
  Ack-eviction + reputation weight; REJECT RTT/GPS wormhole/secure-position;
  DEFER SAND to v2. **CONFLICTS C1–C6 (DISC-0013)** feed DESIGN C-pattern doc
  reconciliation; **gaps G1–G7** (BBPATs→G1, τ→EXP-SEC-001). Workspace
  untouched (research-only; 431 green baseline). **Next: SEC-001 DESIGN**
  (SEC_001_DESIGN.md + ACs + DEC-SEC-0001..N).
- 2026-08-15: **SEC-001 UNDERSTAND COMPLETE (iter 72) — STAGE_TRANSITION to
  RESEARCH.** Threat model synthesised from docs/security/* (adversary classes
  A–F: passive / active-inject-replay / honest-but-curious relay / malicious
  relay blackhole+false-route-ads / compromised / quantum-post; threat actors
  TA-1..9; non-guarantees: anonymity ADVERSARY_MODEL.md:213, availability
  under Adv_M :224, strong Sybil THREAT_MODEL.md:172). AC check: SEC-001 node
  **LACKS `acceptance_criteria`** (PROJECT_GRAPH.yaml:469-475) → C2; DESIGN
  must define per ACCEPTANCE_POLICY `safety_critical_additional` (adversarial
  coverage >95%, fuzzing 24h minimum, external review recommended; DEC-0009
  gate lifted). **DISC-0013 verified**: SYBIL_RESISTANCE.md Defenses 3/4/5
  claim Implemented but no general rate-limiter/reputation/Sybil-detector
  exists; DOS_RESISTANCE.md:213 cites `crates/iris-core/src/node/
  dos_protection.rs` — **path does not exist**; real defenses = dedup
  Bloom+LRU, ForwardedCache, emergency SosRateLimiter/BroadcastReplayGuard,
  trust_store replay counters. SEC-001 generalizes + reconciles docs
  (C-pattern). ADR-0006 per-message-key/key-mgmt already resolved (CRYPTO-001
  iter 55); ADR-0011 open issue (:89) feeds design. Workspace untouched.
- 2026-08-15: **EMERG-001 VERIFY + ACCEPT COMPLETE (iter 71) — node COMPLETE,
  20 COMPLETE nodes.** `docs/implementation/EMERG_VERIFICATION.md` v1.0 AC-1..14
  PASS; independent verifier **APPROVE** (reproduced 431 passed / 0 failed,
  clippy 0, `cargo audit` 0 vulns / 516 crates, all 6 redteam fixes present with
  regression tests, no fake AC evidence); 3 verifier doc-level notes reconciled
  in-pass (provider.rs `BroadcastReplayGuard::MAX_IDS` docstring 3096→1024,
  ignored-test characterization, AC-2 revoked-phrasing). NODE_TRANSITION →
  **SEC-001** (P0 Security Hardening).
- 2026-08-15: **EMERG-001 SECURITY_REVIEW COMPLETE (iter 70)** — EMERG-RT-001..011
  PASS-with-recorded-deviations; RT-001 CRITICAL (provisioned authority root) +
  RT-002 HIGH (sos_rate receive-side enforcement) + RT-006/007/009/011 FIXED
  (+7 regression tests); 431 green.
- 2026-08-15: **EMERG-001 IMPLEMENT COMPLETE (iter 68)** — `emergency/` module
  (11 files) built against real APIs: payload-level `AuthorityMeta` inside the
  signed `EmergencyBroadcast` (KeyAdvertisementV1 reused unchanged as chain
  element per DEC-EMERG-0002), 9-step SPKI/RFC 9804 verify pipeline (root
  required, chain cap ≤4, `Severity::Test` cap-exempt for drills), ciborium CDE
  codec (fixed wire keys, unknown-key tolerant, SOS ≤84 B), SOS classify/cancel
  (60-min same-origin), rate limiter 3/hr/sender (16-B-prefix rolling + CANCEL
  reset), `DisasterMode` guarded_transition ≥15-min holds, bounded
  pseudonymous audit ring, `EmergencyGateway` (TrustStore + limiter + audit),
  engine `set_emergency_provider` seam + `NoopEmergencyProvider` default (AC-11,
  engine unchanged until armed — verified by new tokio engine test
  `emergency_provider_defaults_to_noop_and_switches`). Doc corrections **C1/
  C3/C4/C5/C6 applied (AC-12)**: EMERGENCY_BROADCAST.md (96/96 compact +
  signed-follow-up UPDATE), EMERGENCY_UX.md (WEA 853+960 Hz; SOS resend aligned
  to ack.rs 30 s/2×/unlimited-TTL — no 15-min), EMERGENCY_ABUSE.md (IPC→BNS
  2023 §420→§318, §505→§353(2), §153A→LEGAL-001), EMERGENCY_GOVERNANCE.md
  (X.509-inspired→SPKI-style key-anchored, no PKIX). Build errors iterated to
  green: codec Eq derives (ciborium not Eq), f64 `Triggers` Eq, `&[u8]` limiter
  keys, usize casts, `FieldTooLong(_)` pattern, drill-cap exemption, hand-crafted
  CANCEL wire test. **Workspace 421 green** (366 iris-core incl. 62 emergency +
  seam test; integrations 3+8+4+8+5+3+7+1+13+3 = 0 failed, 1 ignored), clippy 0
  `-p iris-core --all-targets`. STAGE_TRANSITION to TEST. Open item for TEST:
  reconcile `authority_short_id` blake3-impl vs model.rs/design "SHA-256(sender)
  [..16]" wording.
- 2026-08-15: **IDENT-001 ACCEPTED/COMPLETE (iter 63c/64) — 19 COMPLETE
  nodes** — IDENT_VERIFICATION.md v1.0 AC-1..11 all PASS; independent verifier
  APPROVE (reproduced 68 identity passed / 361 workspace passed 0 failed across
  15 suites / clippy 0 --workspace --all-targets / DEC-0010 verified by code
  read engine_handle.rs::build 153-180). Graph IDENT-001 COMPLETE + evidence +
  known_limitations (RT-002/003/004/005 gated, RT-014 accepted);
  PROJECT_STATE completed 19. NODE_TRANSITION to **EMERG-001** (P0, selected
  via PRIORITY_POLICY centrality/risk; critical-path junction).
- 2026-08-15: **IDENT-001 VERIFY COMPLETE (iter 63c)** — IDENT_VERIFICATION.md
  v1.0 AC-1..11 PASS evidence table; verifier APPROVE; 3 doc-level observations
  recorded (68 command-filtered vs 67 module-internal identity tests; unix-gated
  0600 test in TEST record; +10/+12 label drift — net +9..10).
- 2026-08-15: **IDENT-001 SECURITY_REVIEW COMPLETE — PASS-with-recorded-
  deviations (iter 63)** — redteam IDENT-RT-001..016 (5 HIGH / 7 MEDIUM /
  2 LOW / 2 INFO) dispositioned in IDENT-001_SECURITY_REVIEW.md. 9 in-scope
  code findings FIXED in-pass (+10 regression tests, identity unit 58→68):
  RT-001 (adopt_advertisement replay: stale ad → Duplicate drop, never
  permanent KeyChanged downgrade; equal-counter → warn; strictly-higher
  sig-verified → RotationAdopted + clears warning), RT-009 (DEFAULT_SKEW_BUDGET_SECS
  at adopt+rotate), RT-010 (verify_peer exact-key KeyMismatch + un_revoke
  healing), RT-011 (chain format-version gate + RootKeyMismatch consistency),
  RT-012 (RotationError::UnknownPeer), RT-013 (adopt_rotation RED-0011 recheck),
  RT-006 (stale-lock reclaim LOCK_STALE_SECS=15), RT-007 (0600-from-birth temp
  secret file), RT-008 (Zeroizing key-store returns). 4 gated known_limitations
  (RT-002 rotation engine E2E → PROTO-001 v2; RT-003 inbound KeyRotation feed →
  DISCO-001; RT-004 PgStorage::with_sealer desktop deviation; RT-005 Windows
  keystore) + RT-014 provision TOCTOU accepted single-instance + RT-015/016 INFO
  clean. **DEC-0010 security control VERIFIED satisfied**. Workspace **361
  green** (306 iris-core incl. 68 identity), clippy 0. STAGE_TRANSITION to VERIFY.
- 2026-08-15: **IDENT-001 TEST COMPLETE (iter 62b)** — IDENT-001_TEST.md
  AC-1..9 evidence mapped + recorded (peer_id 7, provisioning 13, advertisement
  6, verify_chain 11, TrustStore 10, rotation 7, small-order 4, desktop 3);
  workspace green. STAGE_TRANSITION to SECURITY_REVIEW.
- 2026-08-15: **IDENT-001 IMPLEMENT COMPLETE (iter 62)** — identity module
  (`identity/{provision,store,peer_id,advertise,trust_store,chain,rotate,
  small_order}`: NodeIdentityV1 provision/load, FileKeyStore, PeerId/SHA-256
  short-id, RED-0005 ad, TOFU TrustStore, RED-0008 verify_chain,
  rotation/null-rotation, RED-0011; 58 tests) + `MessageEngine::set_key_directory`
  + desktop RED-0001 (`DesktopIdentity` + IrisCryptoProvider + TrustKeyDirectory
  + persisted PeerId; DevCryptoProvider → test seams; IRIS_NODE_ID UID label; 5
  tests). Workspace green, clippy 0. STAGE_TRANSITION to TEST.
- 2026-08-15: **IDENT-001 DESIGN COMPLETE (iter 61)** —
  `docs/implementation/IDENT_DESIGN.md` v1.0 (D1-D6, AC-1..11, module layout,
  wire reconciliation, RED-0005/0008/0011, RED-0001 desktop wiring plan);
  STAGE_TRANSITION to IMPLEMENT.
- 2026-08-15: **IDENT-001 RESEARCH COMPLETE (iter 60)** — RES-0016 recorded
  (identity model/address derivation, per-platform key provisioning, TOFU+
  pairing trust, rotation/revocation, Ed25519-signed-X25519 binding) +
  DEC-P0005..P0009 resolved. Key evidence: CVE-2025-53627 (never MAC-derived
  identity), KeyDroid StrongBox perf, AOSP HKDF-SHA256 pattern, SPKI RFC 9804,
  KERI null-rotation, Signal X3DH/Meshtastic 2.8.x key-binding precedent,
  RUSTSEC 2026-08-15 posture. Graph/state advanced; STAGE_TRANSITION to
  DESIGN.
- 2026-08-15: **CRYPTO-001 VERIFY + ACCEPT (iter 59)** — CRYPTO_VERIFICATION.md
  v1.0 AC-1..9 all PASS; redteam RED-0001..0012 dispositioned; **RED-0001
  CRITICAL operator-ratified (DEC-0010)** to IDENT-001 with security control;
  CRYPTO-001 **COMPLETE** (18 COMPLETE nodes); NODE_TRANSITION to IDENT-001.
- 2026-08-15: **CRYPTO-001 SECURITY_REVIEW (iter 58)** — redteam FAIL-with-
  condition (RED-0001..0012 recorded in CRYPTO-001_SECURITY_REVIEW.md).
  RED-0004 fragmentation/E2EE FIXED (extended 86-B fragment header, distinct
  wire ids, bind_sender, whole-ADU re-verify) + two latent routing bugs
  (fragmentable selection, MIN-over-ranked MTU); new M7 fragmented-encrypted
  E2E PASS. RED-0002 mitigated (warn+metric+threat model); RED-0006 genuine
  weak-key forgery test; RED-0009 verify→verify_strict; RED-0012 schema
  comment. **RED-0001 CRITICAL (desktop wiring) → OPERATOR GATE.** Workspace
  **290 green**, clippy 0. CHANGELOG 0.3.5/0.3.6 (+ recovery note).
- 2026-08-15: **CRYPTO-001 IMPLEMENT COMPLETE (iter 57)** — full crypto stack
  landed + workspace 284 green, clippy 0. At-rest RowSealer/StorageKeySealer +
  schema v2 (plaintext sender_id/recipient_id + recipient index dropped),
  7 seal unit + 3 pg_store DB tests; RFC 5869 App A.2/A.3 KATs + sender/
  recipient same-key derivation; fixed pre-existing kdf.rs doctest break +
  clippy warning; M7 `crypto_e2e.rs` AC-8 E2E tests (encrypted+signed
  round-trip + tampered frame rejected). Design decisions fully implemented
  (D1–D5).
- 2026-08-14: **CRYPTO-001 DESIGN COMPLETE (iter 56)** — CRYPTO_DESIGN.md v1.0
  with AC-1..9 (module layout, wire/AAD/KDF/at-rest contracts, IrisCryptoProvider
  + KeyDirectory, P0 broadcast non-encryption, KAT line-up, crate pins). FAIL-0004
  (HKDF-BLAKE3 formally recorded) + DISC-0010 (libcrux advisories ≠ RustCrypto
  crates) added; CHANGELOG 0.3.4. Graph CRYPTO-001 RESEARCH→DESIGNING with
  DESIGN COMPLETE stage_note.
- 2026-08-14: **DEC-0009 OPERATOR AUTHORIZATION (iter 54)** — all gates +
  human gates unblocked (standing). BLK-0001 crypto/identity RESOLVED; BLK-0005
  transports RESOLVED for implementation; LEGAL-001/EMERG-001/SEC-001/TEST-001/
  ANDROID-001/IOS-001/PILOT-001 approval gates lifted. Recorded in
  `engineering/memory/records/DEC-0009.md` + registered in ALLOCATION.md.
- 2026-08-14: ML-001 ACCEPT COMPLETE (iter 53) — ML_VERIFICATION.md v1.0
  written; verifier **PASS_WITH_GAPS** (AC-1..7 all PASS + independently
  reproduced: 132 identical feature sequences, byte-identical anchors, GT 0.00
  ms vs static 545.45 ms on periodic_ferry); 3 doc corrections applied
  (AC-5 vehicle_relay/determinism-degeneracy clarity, AC-6 205-core breakdown,
  known-limitations point 5). ML-001 **COMPLETE** in PROJECT_GRAPH.yaml.
  PROJECT_STATE: completed 17, designing 2. Next: TBD (leaf).
- 2026-08-14: ML-001 SECURITY_REVIEW COMPLETE (iter 52) — redteam **PASS**
  (RED-0002): shadow isolation holds (no L3→forwarding path, zero RNG
  coupling, AC-2 byte-identical); ML-RT-02/03 fixed in-pass; ML-RT-01/04/05/
  06/07 → known_limitations. Workspace **244 green**, clippy 0. Next: VERIFY.
- 2026-08-14: ML-001 IMPLEMENT + TEST COMPLETE (iter 52) — shadow-only L3 ML
  prototype in `sim/ml/` (FeatureVec, LinearPredictor, GtPredictor,
  ShadowRecorder) wired into SIM-001 (`with_shadow()`, `shadow_features` at
  the L2 decision point, `SimOutcome.shadow_samples`); `periodic_ferry` +
  `random_walk` scenarios; 10 unit + 8 integration tests **AC 1–6 PASS**.
  Next: SECURITY_REVIEW.
- 2026-08-14: ML-001 RESEARCH COMPLETE (iter 51) — RES-0014 SOTA for L3
  delivery-probability/gateway prediction recorded + registered; graph ML-001
  RESEARCHING→DESIGNING; PROJECT_STATE researching 0/designing 3.
- 2026-08-14: ARCH-001 ACCEPT (iter 50) — ARCHITECTURE_BASELINE.md 7-layer +
  component + 20-principle reconciliation vs implemented system; verifier
  PASS_WITH_GAPS (AC 1-4); ARCH_VERIFICATION.md v1.0; graph ARCH-001 COMPLETE.
  Graph-anomaly fix complete (16 COMPLETE nodes).
- 2026-08-14: REQ-001 ACCEPT (iter 49) — REQUIREMENTS_BASELINE.md 56-req
  matrix (8 IMPLEMENTED / 17 PARTIAL / 30 DEFERRED / 2 GAP); verifier
  PASS_WITH_GAPS (AC 1-4); REQ_VERIFICATION.md v1.0; graph REQ-001 COMPLETE.
- 2026-08-14: WP-10 DESKTOP-001 ACCEPT — Tauri v2 desktop shell COMPLETE.
  DESKTOP_VERIFICATION.md AC 1-6 PASS. `crates/iris-desktop` (DesktopEngine
  host, 4-command IPC, vanilla UI, engine_roundtrip + commands_mock tests).
  Root-caused + fixed the workspace break (stub crate no src targets) and the
  comctl32 v6 manifest crash in test binaries (Tauri #13419: tauri-build
  embeds the manifest only in bins; fix = new_without_app_manifest +
  cargo:rustc-link-arg MANIFEST:EMBED/MANIFESTINPUT). Fixed async-runtime
  nesting (block_on in tokio::test) and the State type-key mismatch
  (constructors return Arc<DesktopEngine>; commands request State<'_, Arc<..>>).
  Workspace 225 green, clippy 0 warnings, app builds + runs.
- 2026-08-14: WP-9 OBS-001 ACCEPT — observability system COMPLETE.
  OBS_VERIFICATION.md AC 1-6 PASS. Observability module (injected lock-free
  MetricsRegistry, 9 `iris.*_total` counters, default-deny), ShortId
  `[u8;8]` privacy-truncated ID type (no-alloc, 16-hex Display), seam
  instrumentation across message engine (9 events + 6 counters), routing,
  SCF, gateway. Red-team PASS (OBS-RT-01..10, MEDIUMs fixed in-pass; taxonomy
  gaps recorded). Workspace 217 green, clippy clean.
- 2026-08-14: WP-8 ROUTE-002 ACCEPT — L2 opportunistic routing (PRoPHET v2 +
  binary spray-and-wait) COMPLETE. ROUTE2_VERIFICATION.md AC 1-6 PASS. Fixed
  role-swap bug (`opportunistic_forward` for src>dst decided GTMX+ from the
  receiver's perspective → ferry withheld all handoffs → L2 16/18 vs L0 17/18,
  AC3 violation; Greater branch now returns (src,dst)). SIM evidence:
  L2 17/18 (94.4%) overhead 1.41 hops 1.00 vs L0 17/18 overhead 3.18 hops
  1.29 — parity at −55.6% overhead, loop-free, deterministic. Workspace 208
  green, clippy clean.
- 2026-08-13: WP-7 GW-001 COMPLETE — gateway discovery & selection with
  red-team hardening (REDTEAM-01..05): `crates/iris-core/src/gateway/mod.rs`
  (GatewayType/Capability/Candidate/Selection, weighted quality score,
  GatewayHealthState machine Healthy/Probation/Failed/HardFailed with
  2-strike escalation, diff-based reconcile over DISCO-001 NeighborTable,
  bounded registry + NaN sanitization + deterministic selection). 19 gateway
  tests (13 core + 6 red-team regression). Workspace 190 green, clippy clean.
  GW_VERIFICATION.md v1.1 AC 1-8 PASS.

## Tests & Build

- `cargo test --workspace`: green (**601** passed / 0 failed / 1 ignored —
  iter 105 verified --all-features; **`transport::wifi_direct` = 16 tests** (7
  transport + 9 serv under the shared filter prefix) incl. GO/GC E2E round-trip
  + sender attribution, DNS-SD discovery, band-restricted fallback, discovery
  re-arm/stop, shutdown lifecycle, bounded single-link table, manager
  registration gating + 9 TXT-record adversarial tests. Iter-101 552 → iter-105
  601 (WIFIDIRECT-001 IMPLEMENT +16; all-features incl. proptest suites).
  `transport::wifiaware` = 19 under the shared filter prefix (held ACCEPT iter
  101); `transport::ble` = 42 incl. 9 RT regressions. SEC-001 all-features
  record remains **532** (verifier-reproduced iter 82, incl. proptest suites;
  all-features/proptest fns are feature-gated).
- `cargo test -p iris-core --features proptest --lib security::`: **101 PASS**
  (68 unit + 33 property functions; verifier-corrected iter 82)
- `cargo tarpaulin -p iris-core --features proptest --lib --out xml`: security
  module **590/617 = 95.6%** (AC-14 target >=95% MET; overall 77.44%) — evidence
  `%TEMP%\opencode\tarpaulin-ac14c\cobertura.xml` (verifier confirmed line-rate
  0.95624)
- `cargo test -p iris-core --lib security::`: 68 PASS
- `cargo clippy --workspace --all-targets`: **0 warnings** (iter 105 verified
  `--workspace --all-features --tests`)
- Postgres required for iris-storage tests: `IRIS_PG_PASSWORD` env var
- `cargo build -p iris-desktop` + `target/debug/iris-desktop.exe` launches
  (windowless smoke check, stays alive)

## Blockers / Gates

- **🚀 ALL GATES UNBLOCKED (DEC-0009, 2026-08-14)** — standing operator
  authorization. No node is blocked by approval. RED-0001 CRITICAL gate
  RESOLVED by operator ratification (DEC-0010, 2026-08-15) — IDENT-001 now
  COMPLETE (iter 63c/64).
- **IDENT-001 security control (DEC-0010)**: no production deployment with
  DevCryptoProvider / no identity / no sealer — **VERIFIED satisfied** in
  IDENT-001 ACCEPT; enforcement carried forward for future platform nodes.
- **BLK-0005 (RESOURCE-scoped)**: physical-device integration tests for
  BLE/WiFi/LoRa/SAT still need real hardware + mobile OS release process —
  recorded as known_limitations, NOT implementation blockers.

## Next Actions

1. **WIFIDIRECT-001 SECURITY_REVIEW (iter 107, active)** — redteam adversarial
   review of `wifi_direct.rs` / `wifi_direct_serv.rs` / transport wiring +
   `WIFI_DIRECT_COST` (pattern BLE-001_SECURITY_REVIEW.md /
   WIFIAWARE-001_SECURITY_REVIEW.md): data paths (GO/GC sender attribution,
   envelope seam gate, per-tick drain budget, outbox bounds, dropped-inbound
   telemetry); concurrency (connect+shutdown race, unavailable gate, reconnect
   reuse, MutexGuard correctness); lifecycle/availability (churn, band fallback,
   re-arm termination, persistent-GO teardown); aux (cost overflow, 255-B TXT
   cap, log hygiene — no secrets). Findings dispositioned → regression tests →
   `WIFIDIRECT-001_SECURITY_REVIEW.md` → VERIFY (iter 108).
2. Transports — WIFIDIRECT-001 (active, SECURITY_REVIEW), then LORA-001, SAT-001,
   BLE-002 (iOS).
3. TEST-001, platform apps — ANDROID-001, IOS-001, PILOT-001.
4. Follow-ups from DESKTOP-001 known_limitations: PgStorage wiring +
   sealer, subscribe_inbox Channel IPC test, relay registration test, CSP
   hardening.

## Risks / Open Items

- **SEC-001 (COMPLETE, iter 82)**: ACCEPTED — AC-1..16 all PASS, verifier
  APPROVE_WITH_NOTES reconciled. Honest-history recap: prior "464 green /
  clippy 0 / AC-14 >=95% / fuzz running" claims were **FALSE** (module did not
  compile) → corrected iter 77. Final verified: **532 workspace green / 0 failed
  (all-features, verifier-reproduced iter 82)**, clippy **0**, **101 security
  tests (68 unit + 33 property)**, **AC-14 590/617 = 95.6% (>=95% MET)**,
  **AC-15 recorded gated deviation operator-ratified** (property-based suite +
  44M+ clean fuzz), **AC-16 grep guard CLEAN**. Real bugs fixed with regression
  tests: `acl::check_sos_identity` Noop-permissive (iter 77); **`quota`
  overflow/wraparound bypass** (iter 78, fuzz-discovered); **redteam SEC-RT 12
  findings** (iter 79); **SEC-RT-15 acl root decode** (iter 80). Background fuzz
  continues clean (44M+ execs) — no blocker. Next: transports (BLE-001 first).
- Requirements baseline reconciled 2026-08-14: 30 DEFERRED (BLK-0001 crypto,
  BLK-0005 hardware, LEGAL-001/EMERG-001 human gates), 2 GAP — no BENCHMARK
  node for REQ-004-05/07 performance targets (proposed follow-up work package)
- Full-size dedup Bloom (~180 KB) requires Fragment path (type 13) for real
  deployments
- SIM-reality gap (virtual ms, enumerated contact schedules) documented in
  SIM_VERIFICATION.md
- Deployment TLS/pooling for Postgres (DEC-0002 follow-ups)
- ROUTE-002: PRoPHET is IRTF Experimental; DP exchange rides DISCO-001
  CapabilityBundle (256 B → top-N snapshot TOP_N_DP=32); per-msg max_dp_seen
  is in-session state (not persisted across reboot)
- OBS-001: route.flood/stored/dropped + scf.awaiting_contact/forward_attempt
  + topo.event defined but not emitted (known_limitations); MetricsRegistry
  reset() is the 7-day retention hook, host must call it
- DESKTOP-001: BLE/Wi-Fi local transport needs BLK-0005 hardware — desktop v1
  binds loopback/Internet transports; MemoryStorage history lost on restart;
  node identity per-process random until IDENT-001
- ML-001: P3 experimental; training offline-only in SIM-001 (synthetic traces
  may not transfer — ML-MaxProp limitation), central-model SOTA results not
  directly transferable to distributed nodes; L3 must never touch the critical
  delivery path (shadow metrics only). Redteam (RED-0002) PASS with
  known_limitations: ML-RT-01 unseeded `Uuid::now_v7()` — same-seed
  determinism claim holds for current scenario shapes only (pre-existing
  sim-wide); ML-RT-04 unbounded ShadowRecorder (harness-only); ML-RT-05/06/07
  INFO (with_shadow replace, age-slot scale, contact-index validation)
- CRYPTO-001 (COMPLETE, iter 59): AC-1..9 PASS; redteam RED-0001..0012
  dispositioned. Known limitations: RED-0003 (unsigned encryption_hdr strip-
  downgrade → PROTO-001 v2/ADR-0011); RED-0010 (codec revision → PROTO-001 v2);
  no recipient-side FS in v1 (v2 X3DH/prekeys); at-rest sealing opt-in until a
  deployment provisions and wires the master key. RESOLVED-BY-IDENT-001:
  RED-0001 (provider/identity/keydir/sealer wiring, security control verified),
  RED-0005 (key binding), RED-0008 (auth_cert_chain), RED-0011 (small-order).
- IDENT-001 (COMPLETE, iter 63c/64): identity/key mgmt/trust/address derivation
  ACCEPTED (AC-1..11 PASS, verifier APPROVE; DEC-0010 control verified
  satisfied). known_limitations: RT-002 (engine multi-key rotation decrypt E2E
  → PROTO-001 v2/RED-0010), RT-003 (inbound KeyRotation feed → DISCO-001),
  RT-004 (PgStorage::with_sealer desktop deviation under DEC-0010), RT-005
  (Windows keystore posture), RT-014 (provision two-file TOCTOU accepted
  single-instance), platform key stores deferred (Android Keystore/iOS
  Keychain/TPM), 16B sender_id wire collapse deferred to PROTO-001 v2,
  TOFU/re-key informal proof gaps, at-rest seal OPT-IN.
- EMERG-001 (COMPLETE, iter 71): emergency system ACCEPTED — AC-1..14 all
  PASS, verifier APPROVE, redteam PASS-with-recorded-deviations (RT-001..011
  dispositioned). known_limitations: BLE 512-B MTU cannot carry verified
  authority chains (2-elem 690 B; SOS P0 stays radio-native), send-side
  envelope synthesis app/authority-owned, typed `EmergencyEvent` OS-surface
  wrappers deferred to platform nodes, RT-003 CANCEL ledger resolution gated,
  RT-004 DisasterMode engine-trigger wiring deferred, RT-005 unarmed-Noop
  intentional, RT-008 drill surface suppression app-owned, RT-010 pre-gate
  audit gap tracked, 17 pre-existing desktop-transitive `cargo audit` warnings
  (gtk/atk/gdk/glib/unic-ucd/proc-macro-error; no iris-core/crypto advisory).
- SEC-001 (COMPLETE, iter 82): general security hardening ACCEPTED — AC-1..16
  all PASS, verifier APPROVE_WITH_NOTES (reconciled). known_limitations:
  **AC-15 recorded gated deviation operator-ratified 2026-08-16** (property-
  based adversarial suite 68 unit + 33 property fns + 44M+ clean fuzz execs;
  literal >=24h wall-clock fuzz not reached — accumulation continues in
  background), AC-14 uncovered = defensive-unreachable + async_trait
  attribution artifact (not coverable), BLE 512-B verified-chain ceiling
  (2-elem security chain 690 B), per-identity rate limits Sybil-defeatable
  (mitigated, not eliminated — RES-0018/Douceur IPTPS 2002), reputation
  sequential convergence (never a gate), Meshtastic-style fixed cadence NOT
  adopted, AC-16 grep guard is a review-trigger (`|| true` by design; test-NAME
  false-positive confirmed).