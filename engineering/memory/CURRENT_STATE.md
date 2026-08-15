# CURRENT_STATE.md

**Schema version**: 1.0
**Last updated**: 2026-08-15T22:05:00Z

---

## Summary

All core-engine + routing + observability work packages (WP-A codec → WP-9
OBS-001), the desktop platform shell (WP-10 DESKTOP-001), the ML routing
experiments (ML-001), the cryptographic layer (CRYPTO-001), the identity
system (**IDENT-001**), and the emergency system (**EMERG-001**) are
implemented, tested, verified, and accepted.
GW-001, ROUTE-002, OBS-001, DESKTOP-001, REQ-001, ARCH-001, ML-001,
CRYPTO-001, **IDENT-001**, and **EMERG-001** are COMPLETE (**20 COMPLETE
nodes**).
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
3 verifier doc-level notes reconciled in-pass. **20 COMPLETE nodes** — the
three P0 SECURITY pillars (CRYPTO-001, IDENT-001, EMERG-001) are all ACCEPTED.
Critical-path junction (SCF → IDENT → EMERG → ANDROID → PILOT) passed.
**🔄** Now **SEC-001 (P0 SECURITY — Security Hardening)**: threat-model
implementation, DoS/replay/Sybil/spam resistance across all message classes.
**UNDERSTAND (72) + RESEARCH (73, RES-0018) + DESIGN (74) COMPLETE** —
`docs/implementation/SEC_001_DESIGN.md` v1.0, **AC-1..16 defined** (resolves C2:
node had no `acceptance_criteria`), DEC-SEC-0001..0008 ratified, RED-0002/RED-
0003/ADR-0011/EMERG-RT-010 inputs absorbed, DISC-0013 doc-vs-code drift
reconciled via matrix. **DESIGN (next) → IMPLEMENT.**

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
| Next node | ✅ SEC-001 (P0 SECURITY) | **UNDERSTAND (72) + RESEARCH (73) + DESIGN (74) COMPLETE → IMPLEMENT.** SEC_001_DESIGN.md v1.0 (module security/{rate_limiter,quota,replay,reputation,spam,acl}; Noop facade; zero new wire + zero new crypto); **AC-1..16 defined** (rate limiter, quota, freshness window, per-sender high-water, cross-reboot persistence, msg-id collision, fragment replay, reputation routing-weight-only, reputation-not-a-gate, spam annotation, ACL-1, Noop compatibility, doc reconciliation, adversarial >=95%, fuzz >=24h, review); DEC-SEC-0001..0008 ratified (token bucket RFC 2697, quota+pool, replay persistence, reputation scope, spam scoring, ACL-1, PoW REJECT, SybilGuard/Limit REJECT); DISC-0013 reconciled. Then transports → TEST-001 → apps |

## Active Node

**SEC-001 — Security Hardening (P0 SECURITY, DESIGN COMPLETE — iter 74 → IMPLEMENT).**
Threat-model implementation, DoS resistance, replay protection, Sybil
mitigation, spam resistance. Fourth P0 SECURITY node (CRYPTO-001, IDENT-001,
EMERG-001 all COMPLETE). Deps CRYPTO-001 + MSG-001 + ROUTE-001 all COMPLETE.
Selected via PRIORITY_POLICY selection_algorithm (P0; deps COMPLETE; lowest-
index eligible after EMERG-001). Quality process: UNDERSTAND ✅ (72) →
RESEARCH ✅ (73) → DESIGN ✅ (74) → IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY →
ACCEPT.
UNDERSTAND consolidated: adversary classes A–F + TA-1..9 (ADVERSARY_MODEL.md;
non-guarantees anonymity :213 / availability-under-Adv_M :224 / strong-Sybil
THREAT_MODEL.md:172); **DISC-0013 doc-vs-code drift verified**
(SYBIL_RESISTANCE Defenses 3/4/5 "Implemented" false; DOS_RESISTANCE.md:213
`dos_protection.rs` path nonexistent; real defenses today = dedup Bloom+LRU +
ForwardedCache + emergency SosRateLimiter/BroadcastReplayGuard + trust_store
replay counters). DESIGN deliverable: SEC_001_DESIGN.md v1.0 (module security/
{rate_limiter,quota,replay,reputation,spam,acl}; Noop facade; **AC-1..16
defined** resolving C2; DEC-SEC-0001..0008 ratified). Key inputs to
operationalize: RED-0002 (do-not-send-
unencrypted fail-closed → ACL-1), RED-0003/EMERG-RT-003/ADR-0011 (signed
encryption_intent), EMERG-RT-010 (pre-gate emergency audit gap), generalized
replay/Sybil/DoS/rate protection across all message classes. Source docs:
`docs/security/*` (13 files), decisions/ADR-0002..0011. CRITICAL findings halt
+ report (RISK_POLICY).

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

- `cargo test --workspace`: green (**421** tests: 366 iris-core incl. 62
  emergency + engine seam + M7 crypto_e2e 3 + 8 obs + 8 sim_scenarios + storage
  25 + ml 5 + desktop 7 + misc; integrations 0 failed, 1 ignored)
- `cargo test -p iris-core emergency`: 62 PASS (emergency module + engine seam)
- `cargo test -p iris-core identity`: 68 PASS (identity module = 67, command
  filter adds `crypto::keygen::tests::ed25519_identity_is_32_bytes`)
- `cargo test -p iris-desktop identity`: 3 PASS
- `cargo clippy -p iris-core --all-targets`: 0 warnings
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

1. **SEC-001 DESIGN (iter 73+, active)** — RESEARCH ✅ (RES-0018, R1–R12).
   Write `docs/implementation/SEC_001_DESIGN.md`: module layout
   (`security/{rate_limiter,storage_quota,replay,reputation,spam,acl}`),
   generalize the engine receive-path gate (emergency_gate pattern) to all
   message classes, wire seams from `dedup.rs`/`ForwardedCache`/
   `emergency/rate_limit.rs`/`emergency/authority.rs`, **define ACs**
   (mandatory — C2: node LACKS `acceptance_criteria`; ACCEPTANCE_POLICY
   safety_critical_additional: adversarial coverage >95%, 24h fuzzing minimum,
   external review recommended; + Meshtastic CVE regression checklist +
   C1–C6 doc reconciliations), formalize DEC-SEC-0001..N in DECISIONS.md.
   Then → IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT.
2. Transports — BLE-001 (Android FFI adapter), BLE-002 (iOS), WIFIAWARE-001,
   WIFIDIRECT-001, LORA-001, SAT-001.
3. TEST-001, platform apps — ANDROID-001, IOS-001, PILOT-001.
4. Follow-ups from DESKTOP-001 known_limitations: PgStorage wiring +
   sealer, subscribe_inbox Channel IPC test, relay registration test, CSP
   hardening.

## Risks / Open Items

- **SEC-001 (active, iter 73)**: RESEARCH COMPLETE (RES-0018) — DOC-VS-CODE
  DRIFT (DISC-0013, conflicts C1–C6) unresolved until DESIGN:
  SYBIL_RESISTANCE.md Defenses 3/4/5 + DOS_RESISTANCE.md:213 `dos_protection.rs`
  claim implemented defenses that do NOT exist in code (emergency-only rate
  limiting today). SEC-001 DESIGN must generalize DoS/replay/Sybil/spam across
  all message classes + correct docs (C-pattern), else the security posture
  documentation overstates reality. SEC-001 node LACKS `acceptance_criteria`
  → DESIGN defines (ACCEPTANCE_POLICY safety_critical_additional: >95%
  adversarial coverage, 24h fuzzing minimum, external security review
  recommended). Replay-across-reboot hole (Bloom persistence deferred WP-2)
  now promoted to SEC-001 requirement (R5).
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