# Engineering Changelog

All significant engineering changes, decisions, and milestones are recorded here.

Format: `## [version] - date - type - description`

Types: ARCHITECTURE | SECURITY | RESEARCH | DECISION | NODE | DOCUMENTATION | POLICY | BUG

---

## [0.3.19] - 2026-08-15 - SECURITY | NODE | DOCUMENTATION

**SEC-001 DESIGN COMPLETE (iter 74) - STAGE_TRANSITION to IMPLEMENT.**

### Added
- **`docs/implementation/SEC_001_DESIGN.md` v1.0** - general per-message-class
  security hardening (P0-P7) extending EMERG-001 defenses. Module layout:
  `crates/iris-core/src/security/{rate_limiter.rs, quota.rs, replay.rs,
  reputation.rs, spam.rs, acl.rs}` + Noop facade seam (AC-12: un-armed engine
  byte-identical). Constraints: zero new wire fields + zero new crypto crates
  (D6 / RES-0018 R11).
- **AC-1..AC-16 defined** on PROJECT_GRAPH SEC-001 node - resolves the C2 gap
  (node entered DESIGN with NO acceptance_criteria): srTCM rate limiter
  (RFC 2697), per-sender quota + priority pool, freshness window + per-source
  skew (RFC 9171 §4.2.7), per-sender high-water (ts,seq), cross-reboot
  persistence (promotes WP-2 Bloom deferral, message_engine/mod.rs:24),
  128-bit msg-id collision (NIST SP 800-107), fragment replay re-entry,
  reputation = routing weight only (CORE positives + iTrust audits),
  reputation-not-a-gate, receiver-side spam annotation, ACL-1 authorization
  (closes RED-0002/RED-0003/ADR-0011/EMERG-RT-003), Noop compatibility,
  doc reconciliation (DISC-0013), adversarial coverage >=95%, fuzzing >=24h,
  Security_Agent review.
- **DEC-SEC-0001..0008 ratified** in `engineering/memory/DECISIONS.md`:
  token-bucket rate limiting, storage quota + priority pool, cross-reboot
  replay persistence, reputation scope (weight-only), receiver-side spam
  scoring, ACL-1, **per-message PoW REJECT** (battery/duty-cycle),
  **SybilGuard/SybilLimit REJECT** (sparse-DTN fast-mixing assumption fails).
- **Doc reconciliation matrix (AC-13)** in SEC_001_DESIGN.md - maps
  DISC-0013 C1-C6 (DOS_RESISTANCE `dos_protection.rs` nonexistent,
  SYBIL_RESISTANCE Defenses 3/4/5 unimplemented, REPLAY_PROTECTION.md SQLite
  + "FP 0%", THREAT_MODEL/SECURITY_ARCHITECTURE reputation) to concrete doc
  edits applied in the same pass.

### Changed
- PROJECT_GRAPH: SEC-001 status RESEARCHING → DESIGNING + acceptance_criteria
  (16) + stage_note DESIGN COMPLETE; validation_status updated to DESIGN
  COMPLETE (iter 74) → next IMPLEMENT.
- PROJECT_STATE: research_complete 2 → 1 (LEGAL-001), designing 0 → 1
  (SEC-001); next_recommended + critical_path annotation updated.
- execution-state.yaml / ACTIVE_NODE.md / NEXT_ACTION.md / CURRENT_STATE.md:
  DESIGN stage COMPLETE → active_stage IMPLEMENT.

### Next
SEC-001 IMPLEMENT - build `security/` module per SEC_001_DESIGN.md against
real APIs (rate_limiter srTCM per (sender,class), quota + priority pool,
replay freshness + high-water + persistence, reputation routing-weight,
spam annotation, ACL-1 over emergency/authority.rs verify pattern), wire
into MessageEngine ingest + deliver_or_relay, then TEST → SECURITY_REVIEW
(redteam SEC-RT) → VERIFY → ACCEPT.

## [0.3.18] - 2026-08-15 - RESEARCH | NODE

**SEC-001 RESEARCH COMPLETE (iter 73) — STAGE_TRANSITION to DESIGN.**

### Added
- **RES-0018 recorded + registered** (`engineering/memory/records/research/
  RES-0018.md`; ALLOCATION.md next RES-0019): 2024–2026 SOTA for DTN/mesh DoS
  resistance, replay protection beyond dedup, Sybil mitigation, spam/abuse
  resistance, wormhole/blackhole — 12 websearch evidence passes,
  evidence-leveled (RFC/NIST L1 · academic L2 · IETF-draft/tech-report L3 ·
  OSS/internal L4), no AI citations; "BBPATs" unresolvable → Gap G1.
- **R1–R12 decisions**: ADOPT srTCM token bucket per (sender,class)
  silent-drop P0/P1-exempt (RFC 2697/2698, RFC 9171 §6.9) + per-sender storage
  quota/priority-reserved pool + TTL-ordered eviction (Claim-Carry-and-Check)
  + Meshtastic CVE regression suite; REJECT per-message PoW (battery/duty-
  cycle/difficulty-calibration), DEFER identity-mint PoW (KeyChallenge/SyDeLP)
  + VDF + RFC 7859 to PROTO-001 v2 (no-new-crypto constraint held); ADOPT
  layered replay (dedup + freshness window + per-sender high-water ts,seq) +
  **Bloom+LRU persistence promoted WP-2 → SEC-001** (cross-reboot closure,
  `mod.rs:24`); ADOPT Ostra link-credit + Briar BQP Sybil layer; REJECT
  SybilGuard/SybilLimit/global reputation on relays; ADOPT reputation as
  routing weight only (CORE-positive second-hand, iTrust audits); ADOPT
  RED-0002 ACL-1 key-anchored per-class sender allowlist (over
  `emergency/authority.rs`); ADOPT receiver-side Bayesian spam scoring only;
  ADOPT blackhole = replication + Ack-eviction + reputation weight; REJECT
  RTT/GPS wormhole + secure-position; DEFER SAND to v2.
- **CONFLICTS C1–C6** (DISC-0013 doc-vs-code drift) mapped for DESIGN
  C-pattern reconciliation (DOS_RESISTANCE `dos_protection.rs` nonexistent;
  SYBIL_RESISTANCE Defenses 3/4/5 unimplemented; REPLAY_PROTECTION.md SQLite
  persistence + "FP 0%" vs ~1%; THREAT_MODEL/SECURITY_ARCHITECTURE reputation
  unimplemented; PoW verdict REJECT-on-TX). **Gaps G1–G7** recorded (τ →
  EXP-SEC-001).
- PROJECT_GRAPH SEC-001 status DISCOVERED→RESEARCHING + stage_note RESEARCH
  COMPLETE + validation_status updated; PROJECT_STATE research_complete
  2 / discovered 10 + next_recommended note.
- Workspace untouched (research-only; 431 green baseline preserved).

### Next
- SEC-001 DESIGN: `docs/implementation/SEC_001_DESIGN.md` + **define ACs**
  (safety_critical_additional: adversarial >95%, 24h fuzz, external review
  recommended) + DEC-SEC-0001..N formalization → IMPLEMENT → TEST →
  SECURITY_REVIEW → VERIFY → ACCEPT.

---

## [0.3.17] - 2026-08-15 - NODE | DOCUMENTATION | RESEARCH

**SEC-001 UNDERSTAND COMPLETE (iter 72) — STAGE_TRANSITION to RESEARCH.**

### Added
- Threat model synthesised from `docs/security/*`: adversary classes A–F
  (passive / active-inject-replay / honest-but-curious relay / malicious relay
  blackhole+false-route-ads / compromised / quantum-post), operational threat
  actors TA-1..9, explicit non-guarantees (anonymity ADVERSARY_MODEL.md:213,
  availability-under-Adv_M :224, strong Sybil THREAT_MODEL.md:172).
- **AC check (confirmed)**: SEC-001 node **lacks `acceptance_criteria`** in
  `PROJECT_GRAPH.yaml` (469-475) → C2 conflict; DESIGN must define ACs per
  ACCEPTANCE_POLICY `safety_critical_additional applies_to [SEC-001]`
  (adversarial_test_coverage >95%, fuzzing_duration 24h minimum,
  external_security_review recommended; human_approval gate lifted by
  DEC-0009, quality retained).
- **DISC-0013 (verified — doc-vs-code drift)**: `SYBIL_RESISTANCE.md` Defenses
  3 (Rate Limiting) / 4 (Anomaly Detection "L2 Intelligence") / 5 (Reputation
  "Partially Implemented") claim implemented defenses, but no general rate
  limiter / reputation / Sybil detector exists in `crates/iris-core`;
  `DOS_RESISTANCE.md:213` cites `crates/iris-core/src/node/dos_protection.rs`
  — **that path does not exist** (`src/node/` absent). Real defenses today:
  dedup Bloom+exact LRU (`message_engine/dedup.rs`), routing `ForwardedCache`
  (`routing/dedup_cache.rs`), emergency `SosRateLimiter` +
  `BroadcastReplayGuard` (`emergency/rate_limit.rs` + `provider.rs`),
  trust_store replay counters (`identity/rotate.rs`).
- SEC-001 inputs to operationalize: RED-0002 (ACL-1 fail-closed), signed
  `encryption_intent` (RED-0003 / EMERG-RT-003 / ADR-0011), EMERG-RT-010
  (pre-gate emergency audit gap).

### Verified
- Workspace untouched (research-only pass): **431 green baseline preserved**,
  clippy 0, cargo audit 0 vulns.
- DISCOVERY DISC-0013 confirmed by glob (no `crates/iris-core/src/node/**`) +
  grep (no dos_protection/sybil_detect/reputation matches in source).

### Registered
- DISC-0013 in `ALLOCATION.md` (next DISC-0014) + `DISCOVERIES.md`.

---

## [0.3.16] - 2026-08-15 - NODE | SECURITY

**EMERG-001 VERIFY + ACCEPT COMPLETE (iter 71) — node COMPLETE; 20 COMPLETE
nodes; NODE_TRANSITION to SEC-001.**

### Added
- `docs/implementation/EMERG_VERIFICATION.md` v1.0 — AC-1..14 PASS evidence
  table (AC-13 redteam verdict from `EMERG-001_SECURITY_REVIEW.md`; AC-14
  live RUSTSEC tree scan + D6 no-new-crypto).
- Independent `verifier` subagent: **APPROVE_WITH_NOTES → reconciled to
  APPROVE**. Independently re-ran `cargo test --workspace` (**431 passed / 0
  failed**, 1 `#[ignore]`d debug helper), `cargo clippy --workspace
  --all-targets` (0 warnings), `cargo audit` (0 vulnerabilities / 516 crates;
  17 warnings all pre-existing desktop-transitive UI crates),
  emergency + message_engine suites, and verified every redteam fix present
  with its regression test (RT-001/002/006/007/009/011) by code read. No fake
  AC evidence.

### Fixed (verifier notes, in-pass)
- `provider.rs` `BroadcastReplayGuard` docstring: `MAX_IDS` 3096 → **1024**
  (matches constant + tests).
- `EMERG_VERIFICATION.md`: corrected ignored-test characterization
  (`route2_debug_undelivered` debug helper, not unix-gated) + AC-2 revoked-
  coverage phrasing (revoked-element rejection path real in `chain.rs`;
  revocation *behavior* proven at trust_store/rotate instruction level).

### Verified
- `cargo test --workspace`: **431 passed / 0 failed** (15 suites).
- `cargo clippy --workspace --all-targets`: **0 warnings**.
- `cargo audit`: **0 vulnerabilities** (516 crates).

### Node completion
- EMERG-001 **COMPLETE** in PROJECT_GRAPH (evidence: EMERG_VERIFICATION.md v1.0,
  EMERG-001_SECURITY_REVIEW.md, EMERG-001_TEST.md, EMERG_DESIGN.md v1.0,
  RES-0017, DEC-EMERG-0001..0008, test counts + audit) + known_limitations
  (BLE 512B ceiling, send-side app-owned, typed EmergencyEvent deferred,
  RT-003/004/005/008/010 recorded, 17 desktop-transitive audit warnings).
- PROJECT_STATE completed **19 → 20**, implementing 1 → 0, security_health
  updated, next_recommended **SEC-001**.
- Critical path junction (SCF → IDENT → EMERG → ANDROID → PILOT) passed.

### Next
- **SEC-001 (P0 SECURITY)** — Security Hardening: threat-model implementation,
  DoS resistance, replay protection, Sybil mitigation. Deps CRYPTO-001 +
  MSG-001 + ROUTE-001 COMPLETE. UNDERSTAND → RESEARCH.
- Then transports (BLE-001/WIFIAWARE-001/WIFIDIRECT-001/LORA-001/SAT-001,
  BLK-0005 device-only tests gated) → TEST-001 → platform apps.

## [0.3.15] - 2026-08-15 - SECURITY | NODE

**EMERG-001 SECURITY_REVIEW COMPLETE (iter 70) — PASS-with-recorded-deviations (AC-13).**
Redteam adversarial review against the emergency receive-path gate. Findings
EMERG-RT-001..011 dispositioned in `engineering/memory/records/EMERG-001_SECURITY_REVIEW.md`.
**FIXED in-pass (+7 regression tests, iris-core 373→376, workspace 424→431):**

### Fixed
- **EMERG-RT-001 (CRITICAL)**: `verify_authoritative` now requires the chain ROOT
  to be a **provisioned authority root** (`TrustStore::is_authority_root`), never
  merely a TOFU-`Unverified` mesh peer — a self-advertising peer can no longer
  anchor a "verified" CRITICAL chain (alert spoofing + relay amplification).
  Plumbed `is_authority_root` + `register_authority_root`; tests
  `tofu_adopted_peer_cannot_be_authority_root` + `provisioned_authority_root_anchors_self_chain`;
  all test chain builders now register roots.
- **EMERG-RT-002 (HIGH)**: `sos_rate` **enforced** in the receive gate before
  bookkeeping — `Allowed`→`Proceed`, `Downgraded`→`SosDowngraded`: envelope demoted
  to P3 degraded delivery (AC-5 receive-side) + `emergency_sos_rate_limited` metric
  + `MSG_EMERGENCY_SOS_RATE_LIMITED` event + `AuditEvent::SosRateLimited` row.
  Limiter bounded `SOS_MAX_BUCKETS` LRU. Tests
  `emergency_gate_downgrades_fourth_sos_to_p3` + `lru_caps_sender_buckets`.
- **EMERG-RT-006 (MEDIUM)**: bounded `BroadcastReplayGuard` (1024 ids / 5-min
  retention) in `EmergencyGateway.verify_envelope` — verified-broadcast replay →
  Drop + `AuditEvent::BroadcastReplayDropped`.
- **EMERG-RT-007 (MEDIUM)**: `AreaOutOfScope` static discriminant — no
  `area_code`/`geo_scope` payload bytes into audit notes (AC-9 content-free).
- **EMERG-RT-009 (LOW)**: poison-safe `RwLock` acquire on the gate path.
- **EMERG-RT-011 (LOW)**: CDE duplicate-key rejection in all emergency maps.

### Recorded known limitations / gated
- RT-003 CANCEL ledger resolution (UnknownOriginal→drop is security-positive;
  wiring gated on the engine ledger seam) · RT-004 DisasterMode engine-trigger
  wiring deferred (library-only AC-7 evidence) · RT-005 AC-11 unarmed-Noop is
  intentional (opt-in only) · RT-008 SOS drills reuse the acceptance path (surface
  suppression app-owned) · RT-010 bad-sig/expired emergency exits predate the gate
  (audit/observability gap tracked).

### Verified
- `cargo test -p iris-core --lib`: **376 passed**; `cargo test --workspace`:
  **431 passed / 0 failed**; `cargo clippy --workspace --all-targets`: **0 warnings**.

### Next
- **EMERG-001 VERIFY** (AC-1..14 evidence → `EMERG_VERIFICATION.md` v1.0 +
  independent verifier reproduction of RT-001/002/006/007/009/011 regression
  tests, seam behavior, 431 green, clippy 0) → ACCEPT → COMPLETE (19 → 20).
  Then SEC-001 → transports → TEST-001 → platform apps.

## [0.3.14] - 2026-08-15 - SECURITY | NODE

**EMERG-001 TEST COMPLETE (iter 69) — per-AC evidence in `EMERG-001_TEST.md`; engine receive-path gate wired (gap closed); size-budget reconciled; STAGE_TRANSITION to SECURITY_REVIEW.**

### Added
- `engineering/memory/records/EMERG-001_TEST.md` — AC-1..12 + AC-14 test-side
  evidence mapping (AC-13 redteam deferred to SECURITY_REVIEW).
- **Engine receive-path gate (gap closed — TEST found + wired)**: the engine
  previously only exposed `set_emergency_provider`; it never invoked the
  provider. Now `deliver_or_relay` consumes the provider when armed:
  - `EmergencyGateOutcome` (EmergencyDropped / EmergencyAlertRelay /
    EmergencyAlertDrillSuppressed / Proceed) + `emergency_gate()` helper in
    `message_engine/mod.rs`.
  - `InboundOutcome::EmergencyDropped` variant; `dropped_emergency_auth`
    `AtomicU64` on `EngineMetrics`/`MetricsSnapshot`.
  - Observability: events `MSG_EMERGENCY_AUTH_DROPPED`,
    `MSG_EMERGENCY_SOS_RATE_LIMITED`, `MSG_EMERGENCY_BROADCAST_RELAYED`,
    `MSG_EMERGENCY_DRILL_SUPPRESSED`; metrics
    `MESSAGES_EMERGENCY_AUTH_DROPPED_TOTAL`, `MESSAGES_EMERGENCY_SOS_RATE_LIMITED_TOTAL`.
  - SOS path: `classify_sos` + `sos_record`/`sos_reset` rate-limit bookkeeping;
    `SosOutcome::Rejected(e)` → drop (no reply).
  - Noop gate = byte-identical pre-emergency behavior (AC-11 regression).
- New engine integration tests: `emergency_gate_drops_unverifiable_alert_no_reply`
  (armed gateway + forged chain → EmergencyDropped, `dropped_emergency_auth ≥ 1`,
  no reply) + `emergency_gate_unarmed_noop_keeps_behavior_identical`
  (Noop → Delivered, `dropped_emergency_auth == 0`).
- Size-budget reconciliation (TEST): measured chain elements **194–196 B**
  (KeyAdvertisementV1: 64 B sig + 2×32 B keys + counter/valid-until + CBOR) →
  2-element verified chain **690 B < 1 KB** (AC-1 budget); full 4-element chain
  **1277 B > 1 KB** = P3 ≥64 KB no-fragment envelope class (regression-guard
  test `four_element_chain_budget_measured_and_fits_p3_envelope`). BLE (512 B)
  cannot carry verified chains — recorded `known_limitation` (SOS P0 stays
  radio-native).
- `authority_short_id` blake3-vs-SHA-256 open item **reconciled**: SHA-256
  matches `identity::peer_id::peer_short` (no code change needed).

### Changed
- `broadcast.rs`: `four_element_chain_budget_measured_and_fits_p3_envelope`
  (replaces the assumption-based 1-KB-fit test: asserts 4-elem >1024 B guard +
  2-elem <1024 B); `oversized_payload_rejected_no_silent_truncation` restored;
  debug `eprintln!`s removed; `build_four_chain_depth(depth)` helper.
- `EMERG_DESIGN.md`: §4 size geometry corrected to measured chain-element
  sizes, §2.3 P3-budget note, AC-1 wording; §10 engine-hook note updated
  (receive-path gate now wired; send-side envelope synthesis + typed
  `EmergencyEvent` protocol deferrals recorded as known_limitations).
- `message_engine/mod.rs` + `observability/mod.rs`: gate + observability
  constants above.

### Next
- **EMERG-001 SECURITY_REVIEW** (AC-13 redteam + `cargo audit` RUSTSEC scan) →
  VERIFY → ACCEPT → COMPLETE (19 → 20). Then SEC-001 → transports → TEST-001 →
  platform apps.

### Verified
- `cargo test -p iris-core emergency`: **65 passed**; `message_engine`:
  **42 passed**; `cargo test --workspace`: **424 passed / 0 failed** (15 suites);
  `cargo clippy -p iris-core --all-targets` + `--workspace`: **0 warnings**.

### Known limitations (recorded for SECURITY_REVIEW)
- BLE 512 B MTU cannot carry verified authority chains (e.g., 690 B 2-elem);
  SOS P0 remains radio-native; verified broadcasts ride P3 ≥64 KB transports.
- Send-side emergency envelope synthesis is app/authority-owned (engine does
  not build `EmergencyAlert` envelopes).
- Typed `EmergencyEvent` OS-surface wrappers deferred to platform nodes
  (ANDROID-001 / IOS-001 / DESKTOP-001).

## [0.3.13] - 2026-08-15 - SECURITY | NODE

**EMERG-001 IMPLEMENT COMPLETE (iter 68) — `emergency/` module + engine seam + doc corrections C1/C3/C4/C5/C6 applied; STAGE_TRANSITION to TEST.**

### Added
- `crates/iris-core/src/emergency/` module (11 files):
  - `model.rs` — `AuthorityMeta` (area/functional/severity/role/max_severity,
    decode/wire caps), `EmergencyBroadcast` payload carrying `authority:
    AuthorityMeta`, severity affine caps, `validate()` via authority.
  - `codec.rs` — ciborium CDE codec mirroring `protocol/codec.rs`: fixed wire
    keys (B_1..15 / A_1..4 / S_1..9), `EmergencyCodecError` (String variants,
    no `Eq` — ciborium errors aren't `Eq`), unknown-key tolerant decode, SOS
    ≤84 B test, TooLarge no-silent-truncation.
  - `authority.rs` — 9-step `verify_authority` pipeline against real IDENT-001
    APIs: `verify_chain(chain, trust, sender_id: &[u8])`,
    `TrustStore::adopt_advertisement`, `KeyAdvertisementV1::from_bytes`; root
    required, chain cap ≤4, depth/validity/small-order/sender-match, SPKI/
    RFC 9804 profile via payload-level `AuthorityMeta`; **`Severity::Test`
    exempt from max_severity cap** so drills pass (DEC-EMERG-0008).
  - `sos.rs` — `classify_sos` → `SosOutcome` (Ok/Drill/Expired/Stale/Cancel),
    60-min same-origin cancel window.
  - `rate_limit.rs` — `SosRateLimiter` keyed by `[u8;16]` peer prefix (16-B
    privacy), rolling 3/3600 s, 4th → downgrade, reset on verified CANCEL.
  - `mode.rs` — `DisasterMode` (Normal→Emergency→Crisis→Degraded→Normal) with
    `guarded_transition` ≥15-min holds (`ESCALATION_HOLD_SECS`=900), f64
    `Triggers` (SOS density / gateway degradation / rate surge).
  - `audit.rs` — bounded pseudonymous metadata-only `AuditLog` ring
    (`EmergencyAuditRecord`; no payload content).
  - `drill.rs` — `is_drill` / `surface_decision` (drills never on OS surfaces).
  - `broadcast.rs` — `verify_and_classify` → Relay/Suppressed/Drop,
    `is_emergency_content`, EMERGENCY_BROADCAST recipient (mandatory relay,
    anti-probing no-reply on unverifiable).
  - `provider.rs` — `EmergencyProvider` trait + `NoopEmergencyProvider` +
    `EmergencyGateway` (TrustStore + `Mutex<SosRateLimiter>` +
    `Mutex<AuditLog>`).
  - `mod.rs` — re-exports (incl. `EmergencyProvider`, `EmergencyGateway`,
    `NoopEmergencyProvider`).
- **Engine seam (AC-11)**: `MessageEngine.emergency` =
  `RwLock<Arc<dyn EmergencyProvider>>`, default `NoopEmergencyProvider`, new
  `set_emergency_provider()`; tokio engine test
  `emergency_provider_defaults_to_noop_and_switches` (mechanical no-op until
  armed).
- **Doc corrections AC-12**: EMERGENCY_BROADCAST.md (96/96 compact +
  signed-follow-up UPDATE — C1), EMERGENCY_UX.md (WEA 853+960 Hz ×2 — C3; SOS
  resend aligned to ack.rs 30 s/2×/unlimited-TTL, no 15-min — C5),
  EMERGENCY_ABUSE.md (IPC→BNS 2023 §420→§318, §505→§353(2), §153A→LEGAL-001 —
  C4), EMERGENCY_GOVERNANCE.md (X.509-inspired→SPKI-style key-anchored cert,
  no PKIX/rustls-webpki — C6).

### Changed
- `lib.rs`: `pub mod emergency;`. Test counts: workspace **421 green** (was
  361); `cargo test -p iris-core emergency` = **62 passed**; clippy 0
  `-p iris-core --all-targets`.

### Known / open (for TEST stage)
- Reconcile `authority_short_id` implementation (blake3) vs model.rs/design
  "SHA-256(sender)[..16]" doc wording.

### Next
- **EMERG-001 TEST** (per-AC evidence → `EMERG-001_TEST.md`) →
  SECURITY_REVIEW (redteam, AC-13) → VERIFY → ACCEPT → COMPLETE (19 → 20).
  Then SEC-001 → transports → TEST-001 → platform apps.

### Verified
- `cargo test --workspace`: **421 green** (0 failed, 1 ignored) — 366 iris-core
  (incl. 62 emergency + engine seam) + integrations; `cargo test -p iris-core
  emergency`: 62 passed; `cargo clippy -p iris-core --all-targets`: 0 warnings.

## [0.3.12] - 2026-08-15 - SECURITY | NODE

**IDENT-001 VERIFY + ACCEPT COMPLETE (iter 63c + 64) — node COMPLETE; 19 COMPLETE nodes; NODE_TRANSITION to EMERG-001.**

### Added
- `docs/implementation/IDENT_VERIFICATION.md` v1.0 — AC-1..11 all PASS with
  full evidence table: AC-1 peer_id derivations (7 tests); AC-2 provisioning
  round-trip/corrupt-loud/zeroize (provision 6 + store 8); AC-3 KeyAdvertisement
  build/verify (6 tests); AC-4 verify_chain matrix (13 tests, cap 8); AC-5
  TrustStore TOFU/rotation/revocation (14 tests); AC-6 rotation policy (9
  tests, earliest-seen-wins); AC-7 RED-0011 (4 tests); AC-8 desktop RED-0001
  (IrisCryptoProvider + FileKeyStore + persisted PeerId; DevCryptoProvider
  confined to config.dev); AC-9 engine E2E + set_key_directory seam; AC-10
  redteam PASS-with-recorded-deviations + DEC-0010 verified; AC-11 this record.
- **Independent verifier (subagent) APPROVE** — reproduced: `cargo test -p
  iris-core identity` 68 passed; `cargo test --workspace` 361 passed / 0 failed
  across 15 suites; `cargo clippy --workspace --all-targets` 0 warnings; DEC-0010
  verified by code read (`engine_handle.rs::build` 153-180: production path
  config.dev==false → IrisCryptoProvider + TrustKeyDirectory from persisted
  DesktopIdentity); all 9 redteam fixed findings present with regression tests;
  no AC row cites a nonexistent test.
- PROJECT_GRAPH: IDENT-001 status COMPLETE + evidence (7) + known_limitations
  (8); PROJECT_STATE: completed 18→19, verifying 1→0, high_risk IDENT-001
  COMPLETE, critical_path junction passed, next_recommended EMERG-001.

### Changed
- Identity security review refined in doc/records: identity module = 67 unit
  tests (command-filtered total 68 includes
  `crypto::keygen::tests::ed25519_identity_is_32_bytes`); +10 regression-test
  label reconciled against 12 named cases (net module delta +9..10 vs the 58
  baseline).

### Next
- **EMERG-001** (P0, deps MSG-001 + IDENT-001 + ROUTE-001 COMPLETE): SOS /
  emergency broadcast / P0-P3 priority routing / disaster mode. Selected via
  PRIORITY_POLICY (dependency_centrality unblocks ANDROID-001 + IOS-001 +
  PILOT-001; risk_reduction safety-critical). UNDERSTAND → RESEARCH → DESIGN →
  IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT.

### Verified
- `cargo test --workspace`: **361 green** (0 failed); `cargo test -p iris-desktop`:
  5+3+3 passed; `cargo clippy --workspace --all-targets`: 0 warnings.

## [0.3.11] - 2026-08-15 - SECURITY | NODE

**IDENT-001 TEST + SECURITY_REVIEW COMPLETE (iter 62b + 63) — redteam IDENT-RT-001..016 dispositioned; PASS-with-recorded-deviations; STAGE_TRANSITION to VERIFY.**

### Added
- `engineering/memory/records/IDENT-001_TEST.md` — AC-1..9 evidence (peer_id 7,
  provisioning 13, advertisement 6, verify_chain 11, TrustStore 10, rotation 7,
  small-order 4, desktop RED-0001 3).
- `engineering/memory/records/IDENT-001_SECURITY_REVIEW.md` — redteam findings
  IDENT-RT-001..016 (5 HIGH / 7 MEDIUM / 2 LOW / 2 INFO) + dispositions +
  verdict **PASS-with-recorded-deviations**.
- +10 security regression tests (identity unit **58 → 68**):
  - `trust_store.rs` — `stale_counter_earliest_seen_wins` (Duplicate/earliest-
    seen-wins), `equal_counter_different_key_warns_but_recoverable`,
    `expired_within_skew_budget_accepted`, `expired_ad_rejected`,
    `verify_peer_rejects_key_mismatch`, `un_revoke_allows_re_adoption_after_heal`,
    `revocation_blocks_all_paths_and_never_auto_heals` (extended).
  - `chain.rs` — `root_certified_key_must_match_store`, `wrong_format_version_link_rejected`;
    `expired_link_rejected` fixed to reuse the store's root key.
  - `store.rs` — `stale_lock_file_is_reclaimed`, `all_key_files_are_0600_from_birth`.
  - `rotate.rs` — `unknown_peer_rotation_rejected`, `adopt_rotation_defensive_small_order_recheck`.

### Changed (in-scope redteam fixes)
- **RT-001 (HIGH)** `TrustStore::adopt_advertisement` (crates/iris-core/src/
  identity/trust_store.rs) — replay of a stale-counter advertisement with a
  different key can no longer permanently downgrade a healthy peer to
  `KeyChanged`: stale same/different-key → `Duplicate` (drop); equal-counter
  different-key → `KeyChangeWarn` (still recoverable); strictly-higher,
  signature-verified counter → `RotationAdopted` **and clears `KeyChanged`**
  (recovery path). Earliest-seen-wins is preserved.
- **RT-009** — `valid_until` skew budget `DEFAULT_SKEW_BUDGET_SECS` (300,
  RFC 9171 §4.4.2) applied at `adopt_advertisement` + rotation `apply`:
  rejected only when `valid_until + 300 < now`.
- **RT-010** — `TrustKeyDirectory::verify_peer` now confirms the **exact**
  expected X25519 static key (`expected_static_x25519_pubkey` param, new
  `VerifyError::KeyMismatch`); added `un_revoke` for operator healing — key
  stays zeroed, fresh ad/rotation required; re-adoption after heal =
  `RotationAdopted` (not `Refreshed`).
- **RT-011** `verify_chain` (chain.rs) — per-element `format_version` gate
  (`ChainError::Verdict`), root-vs-store consistency (`ChainError::RootKeyMismatch`);
  RED-0011 small-order evaluated **before** consistency.
- **RT-012** — rotation for an identity with no TOFU-bound entry →
  `RotationError::UnknownPeer`.
- **RT-013** — `adopt_rotation` now re-checks RED-0011 internally and returns
  `Result<bool, &'static str>`; `rotate.rs` `apply` expects the invariant.
- **RT-006** — `FileKeyStore` lock reclaim: locks with mtime older than
  `LOCK_STALE_SECS` (15 s) are treated as stale and reclaimed (bounded, never
  clobbers a live lock).
- **RT-007** — `write_secret_file` creates the temp file **0600 from birth**
  (OpenOptions mode on Unix) + `truncate(true)` reuse of leftover `.tmp`.
- **RT-008** — zeroize discipline: `KeyStore::load_identity` →
  `Option<Zeroizing<Vec<u8>>>`, `load_master_key` → `Option<Zeroizing<[u8;32]>>`,
  `NodeIdentityV1::to_bytes` → `Zeroizing<Vec<u8>>` (callers unchanged via deref).
- `resolve_x25519` also filters small-order (RED-0011 at read boundary).

### Recorded known_limitations / gated (not silently dropped)
- RT-002: engine multi-key rotation decrypt E2E → PROTO-001 v2 / RED-0010;
  AC-6 scoped to the primitive.
- RT-003: inbound `KeyRotation` feed → DISCO-001.
- RT-004: `PgStorage::with_sealer` desktop wiring deviation (loopback
  MemoryStorage) recorded under DEC-0010; `master_key()` exposed for the future
  PG path.
- RT-005: Windows keystore posture documented (FileKeyStore v1).
- RT-014: provision two-file TOCTOU — accepted single-instance risk.
- RT-015/016 INFO confirmed clean: `verify_strict` everywhere; no PKIX / no new
  crypto crates (RUSTSEC posture clean).
- **DEC-0010 security control VERIFIED satisfied** — no production deployment
  with DevCryptoProvider / no identity / no sealer.

### Next
- IDENT-001 VERIFY (iter 63): `docs/implementation/IDENT_VERIFICATION.md`
  AC-1..11 + verifier dispatch → ACCEPT → COMPLETE → transports.

### Verified
- `cargo test --workspace`: **361 green** (306 iris-core incl. 68 identity +
  3 desktop identity).
- `cargo clippy --workspace --all-targets`: 0 warnings.

## [0.3.10] - 2026-08-15 - SECURITY | NODE

**IDENT-001 IMPLEMENT COMPLETE (iter 62) — identity module + RED-0001 desktop wiring; STAGE_TRANSITION to TEST.**

### Added
- `crates/iris-core/src/identity/` (58 unit tests):
  - `provision.rs` — `NodeIdentityV1` versioned blob (format_version, reserved,
    created_unix, ed25519_seed, static_x25519_secret, key_gen_counter);
    provision-on-first-run / load; corrupt + unknown-version **fail loudly,
    never silent regen** (§5.4); `IdentityManager::provision_or_load` +
    `peer_id`/`peer_short`/`human_uid`/`master_key`/`runtime_node_identity`;
    zeroize-on-drop proven via allocation-wipe test.
  - `store.rs` — `KeyStore` trait + `FileKeyStore` (app-data `iris/keys/`,
    atomic temp+rename, `create_new` advisory lock, 0600 Unix, header-stripped
    load, `Zeroizing` buffers).
  - `peer_id.rs` — PeerId = Ed25519 verifying bytes (self-authenticating);
    `peer_short()` = SHA-256(pubkey)[..16]; base32 human UID; no-MAC rule.
  - `advertise.rs` — RED-0005 `KeyAdvertisementV1` SPKI-style (CBOR subject
    block ‖ 64-byte sig; build rejects small-order; `verify_strict`).
  - `trust_store.rs` — `TrustStore` TOFU (AdoptionOutcome taxonomy, earliest-
    seen-wins, monotonic rotation gate, revocation blocks all, `verify_peer`,
    `TrustKeyDirectory` impl of the engine `KeyDirectory` seam).
  - `chain.rs` — RED-0008 `verify_chain` (key-anchored, trusted root, cap ≤8,
    rejects broken/expired/revoked/small-order; **no PKIX**).
  - `rotate.rs` — `RotationEventV1` rotation/revocation (monotonic counter,
    valid-until, null-rotation revoke, earliest-seen-wins).
  - `small_order.rs` — RED-0011 known low-order U coordinates + `is_small_order`.
- `MessageEngine::set_key_directory` — engine recipient-key lookup is now
  directory-backed (CRYPTO-001 `KeyDirectory` seam).
- Desktop RED-0001 (§11): `crates/iris-desktop/src/identity.rs` `DesktopIdentity`
  (provision_or_load via `FileKeyStore::default_data_dir`, persisted PeerId,
  `IrisCryptoProvider`, master_key for future StorageKeySealer,
  `TrustKeyDirectory`, `adopt_key_advertisement`; 3 tests incl. real
  Ed25519/X25519 sign/verify/encrypt/decrypt with `authenticates() == true`).

### Changed
- `engine_handle.rs` production path: `IrisCryptoProvider` +
  `set_key_directory(TrustKeyDirectory)` + `node_id` = persisted PeerId.
  `DevCryptoProvider` confined to `config.dev` test seams
  (`with_node_id`/`with_transport`); `IRIS_NODE_ID` documented display-only
  UID label (DEC-0010 control satisfied).
- PROJECT_GRAPH.yaml IDENT-001 status DESIGNING→IMPLEMENTING, stage_note
  IMPLEMENT COMPLETE; PROJECT_STATE reconciled (implementing 1 / designing 0).

### Next
- IDENT-001 TEST (iter 62): AC-1..11 evidence → SECURITY_REVIEW → VERIFY →
  ACCEPT.

### Verified
- Workspace green (296+ core incl. 58 identity + 5 desktop identity tests),
  clippy 0 warnings (iris-core + iris-desktop), `cargo check` clean.

## [0.3.9] - 2026-08-15 - SECURITY | DECISION | NODE

**IDENT-001 DESIGN COMPLETE (iter 61) — IDENT_DESIGN.md v1.0; STAGE_TRANSITION to IMPLEMENT.**

### Added
- `docs/implementation/IDENT_DESIGN.md` v1.0: D1-D6 per DEC-P0005..0009 —
  key-derived PeerId (Ed25519 pubkey, 32B self-authenticating) + `peer_short()`
  = SHA-256(pubkey)[..16]; per-platform provisioning (desktop v1 = protected
  `FileKeyStore` 0600/atomic/lock/zeroize; `keyring` OS-keychain deferred as
  RUSTSEC-clean hardening) + wrapped-DEK + `StorageKeySealer` default-on; TOFU
  + signed key advertisement (`KeyAdvertisementV1`, SPKI/RFC 9804) + Verified
  tier; signed rotation/revocation (`KeyRotation` path, monotonic counter +
  valid-until + earliest-seen-wins + KERI null-rotation); two-keypair
  Ed25519-signed-X25519 binding (no XEdDSA); no new crypto crates. Module
  layout `identity/{provision,store,peer_id,advertise,trust_store,chain,rotate,
  small_order}`; RED-0008 `verify_chain` (key-anchored, cap ≤8, no PKIX —
  rustls-webpki family excluded); RED-0011 small-order rejection; AC-1..11.

### Changed
- Wire reconciliation (§3): standard-envelope `sender_id` stays **32 B Ed25519
  pubkey** (self-authenticating — CRYPTO-001 `verify()` unchanged); 16 B
  SHA-256 short-id implemented + table-tested now for P0 abbreviated envelopes;
  **standard-envelope collapse to 16 B deferred to PROTO-001 v2 / RED-0010**
  (needs key-directory pubkey resolution).
- RED-0001 desktop wiring plan: `engine_handle.rs:145` `DevCryptoProvider` →
  `IrisCryptoProvider` + FileKeyStore identity + master-key `with_sealer` when
  PG configured + `IRIS_NODE_ID` → non-authoritative UID label.
- ADDRESSING.md legacy BLAKE3 derivation superseded (patch in IMPLEMENT pass).
- PROJECT_GRAPH.yaml: IDENT-001 stage_note DESIGN COMPLETE (iter 61);
  validation_status; PROJECT_STATE high_risk note.

### Next
- IDENT-001 IMPLEMENT (iter 61): identity module + desktop RED-0001 wiring
  (AC-1..11) → TEST → SECURITY_REVIEW → VERIFY → ACCEPT.

### Verified
- Workspace 290 green, clippy `-D warnings` 0 (carried from iter 59; identity
  module not yet implemented).

## [0.3.8] - 2026-08-15 - RESEARCH | DECISION | NODE

**IDENT-001 RESEARCH COMPLETE (iter 60) — RES-0016 + DEC-P0005..P0009; STAGE_TRANSITION to DESIGN.**

### Added
- `engineering/memory/records/research/RES-0016.md`: 2026 SOTA for identity/key
  mgmt/trust/address derivation — never MAC-derived identity (CVE-2025-53627
  Meshtastic NodeNum spoofing), libp2p PeerId / did:key / Tor onion v3
  key-derived precedents; per-platform provisioning (Android Keystore
  TEE/StrongBox with KeyDroid perf data, iOS Keychain SE P-256-only, desktop
  OS-keychain/file, Pi external TPM sealing-only, Stronghold "not yet audited");
  TOFU + Briar BQP QR/pairing verified tier + SPKI RFC 9804 keyholder chains;
  signed rotation events + KERI null-rotation revocation; Ed25519-signed-X25519
  binding precedent (Signal X3DH/XEdDSA, Meshtastic 2.8.x, IACR 2021/509, LSEG
  arXiv:2511.07548); RUSTSEC 2026-08-15 posture (avoid PKIX family).

### Changed
- DECISIONS.md: **DEC-P0005** (per-device Ed25519 + key-derived sender_id),
  **DEC-P0006** (per-platform provisioning + wrapped-DEK + StorageKeySealer
  default-on), **DEC-P0007** (TOFU + signed advertisement + QR tier, SPKI not
  PKIX), **DEC-P0008** (signed rotation events + null-rotation revocation),
  **DEC-P0009** (two-keypair Ed25519-signed-X25519 binding, no XEdDSA v1) —
  all RESOLVED.
- PROJECT_GRAPH.yaml: IDENT-001 stage_note RESEARCH COMPLETE; validation_status.
- PROJECT_STATE.yaml: IDENT-001 high_risk note + next_recommended (ACTIVE,
  RESEARCH COMPLETE); CRYPTO-001 removed from next_recommended.
- ALLOCATION.md: RES-0016 registered (next RES-0017).

### Next
- IDENT-001 DESIGN (iter 60): `docs/implementation/IDENT_DESIGN.md` v1.0 +
  AC-1..N per DEC-P0005..P0009; absorbs RED-0001 (desktop provider wiring +
  persisted identity + key dir + sealer), RED-0005, RED-0008, RED-0011 →
  IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY → ACCEPT.

### Verified
- Workspace 290 green (unchanged — research-only pass), clippy `-D warnings` 0.

---

## [0.2.0] - 2026-08-11 - ARCHITECTURE | SECURITY | POLICY

**Architecture Validation & Graph Integrity Pass ΓÇö expert review findings addressed.**

### Critical Fixes

**Security ΓÇö Cryptographic documentation corrected:**
- `ADR-0002`: Corrected Ed25519 SHA-512 claim (was incorrectly stated as "BLAKE3 in IRIS's case"; Ed25519 uses SHA-512 internally per RFC 8032; IRIS does not change this)
- `ADR-0006`: Added ΓÜá∩╕Å section documenting three open architecture issues requiring resolution BEFORE implementation:
  1. Per-session vs per-message ephemeral key inconsistency (ADR says "per session"; MESSAGE_ENVELOPE shows per-message header ΓÇö materially different)
  2. "HKDF-BLAKE3" is a non-standard construction ΓÇö requires decision (HKDF-SHA256 or BLAKE3 native KDF)
  3. Current ECIES-like construction does NOT provide forward secrecy against recipient key compromise ΓÇö must be accurately documented; do not claim forward secrecy without qualifying it
- **IMPLEMENTATION GATE: CRYPTO-001 remains BLOCKED pending human cryptographic architecture review resolving the above**

**Regulatory ΓÇö LoRa duty cycle override removed:**
- `docs/transports/LORA.md`: Removed "P0 SOS messages ignore duty cycle tracking" ΓÇö this was a regulatory violation. P0 messages are prioritized in queue but MUST NOT bypass WPC duty cycle limits. Alternative transports used when LoRa budget exhausted.

**Regulatory ΓÇö India spectrum frequency corrected:**
- `docs/transports/LORA.md` + `docs/legal/INDIA_COMPLIANCE.md`: Flagged 865ΓÇô867 MHz ΓåÆ 865ΓÇô868 MHz per 2021 WPC Gazette. All spectrum claims require validation against current Gazette before production.

**Legal ΓÇö Fabricated organizational fact removed:**
- `docs/legal/INDIA_COMPLIANCE.md`: Removed "IRIS has engaged qualified Indian telecommunications legal counsel" ΓÇö this was a fabricated fact. Replaced with: "Legal counsel review is required before any commercial or government deployment."

**Legal ΓÇö DPDP Rules 2025 added:**
- `docs/legal/INDIA_COMPLIANCE.md`: Added Section 2A documenting the Digital Personal Data Protection Rules 2025 (notified by MeitY, November 2025). All legal compliance work must address both the Act 2023 and the Rules 2025.

### Architecture Improvements

**Graph integrity ΓÇö Missing edges added:**
- `engineering/PROJECT_GRAPH.yaml`: Added 18 missing edges that were declared in node dependencies but absent from the edges section. Affected: WIFIDIRECT-001, SEC-001, SIM-001, ANDROID-001, IOS-001, DESKTOP-001, OBS-001, TEST-001, LEGAL-001.
- Graph version bumped to 0.2.0.

**Project state ΓÇö Corrected to match graph:**
- `engineering/PROJECT_STATE.yaml`: Corrected state counts to match actual graph (was: 1 complete, 3 in_progress, 28 discovered; now: 1 complete, 3 research_complete, 4 designing, 24 discovered). Added note that this file must be generated from graph, not maintained independently.

**Platform capability model ΓÇö Promoted to first-class:**
- `docs/architecture/PLATFORM_CAPABILITY_MODEL.md`: New document establishing iOS/Android background execution constraints as architecture facts, not footnotes. Defines four relay capability tiers. Architecture must never treat iOS as equivalent Android relay.

### Policy Improvements

**Evidence maturity model added:**
- `engineering/RESEARCH_POLICY.yaml`: Added 11-level evidence maturity model (HYPOTHESIZED ΓåÆ CERTIFIED). Every technical claim must be tagged. Defines what level of evidence is required for different claim types (regulatory claims require CERTIFIED, performance benchmarks require HARDWARE_VALIDATED, range claims require HARDWARE_VALIDATED, etc.).

**Formal invariants defined:**
- `engineering/FORMAL_INVARIANTS.yaml`: New file. Defines 12 system invariants across security, routing, deduplication, storage, privacy, and emergency categories. Each invariant has a test type, test ID, and fuzz target requirement.

**New agents added:**
- `engineering/AGENT_REGISTRY.yaml`: Added Independent_Verification_Agent (verifies node correctness independently of builder) and Red_Team_Architect (adversarially stress-tests the architecture to find failure modes before implementation).

### Known Open Issues (not yet resolved ΓÇö require follow-up)

1. **Graph size**: 32 nodes insufficient for full system coverage; need 50+ additional nodes for protocol conformance, platform details, security properties, CI/CD, supply chain, product features
2. **Crypto architecture**: per-session vs per-message ephemeral key unresolved; HKDF-BLAKE3 non-standard; forward secrecy properties need clarification
3. **Dependency model**: typed dependencies ({node, relationship, required_state}) not yet implemented in node schema
4. **Supply chain security**: no graph branch for SBOM, dependency pinning, artifact signing
5. **Protocol conformance testing**: no PROTOCOL_CONFORMANCE node
6. **Competitive analysis**: needs primary-source confidence levels (Documented/Observed/Reported/Inferred/Unknown)
7. **Reproducibility**: benchmark documentation needs hardware/OS/seed fields mandated in CI

---

## [0.1.0] - 2025-01-01 - DOCUMENTATION

**Initial documentation and engineering control plane created.**

### Added
- Full documentation tree (`/docs`) covering all subsystems
- Root project documents: charter, vision, problem definition, use cases, system boundaries, glossary
- Architecture documentation: system architecture, reference architecture, layer model, node model, network graph, temporal graph, data flow, control flow, failure architecture, gateway architecture, edge architecture, architectural principles
- Protocol documentation: overview, message model, addressing, message envelope, delivery policies, priority model, TTL, ACKs, retries, deduplication, fragmentation, reassembly, synchronization, versioning, compatibility
- Transport documentation: abstraction, BLE, Bluetooth Mesh, Wi-Fi Direct, Wi-Fi Aware, Wi-Fi, Ethernet, Cellular, Internet, LoRa, Satellite, USB, Future Transports
- Routing documentation: architecture, requirements, baseline, multipath, opportunistic, DTN, store-carry-forward, gateway selection, network healing, congestion control, routing security, experiments
- Identity documentation: architecture, cryptographic identity, key management, trust model, authorization, revocation, verified entities, privacy model
- Security documentation: architecture, threat model, attack surface, adversary model, Sybil resistance, replay protection, message authentication, routing attacks, DoS resistance, spam resistance, emergency abuse, privacy threats, security testing
- Safety documentation: safety charter, responsible use, abuse prevention, emergency governance, authority verification, public channel moderation, incident response, misinformation, law enforcement requests, safety risk register
- Emergency documentation: architecture, SOS, emergency broadcast, priority delivery, location sharing, disaster modes, crowd management, search and rescue, emergency scenarios
- Intelligence documentation: architecture, deterministic layer, graph intelligence, statistical layer, ML layer, RL layer, LLM layer, model evaluation, AI safety
- Simulation documentation: architecture, network simulator, mobility models, failure models, traffic models, satellite simulation, LoRa simulation, scale testing, simulation validation
- Platform documentation: architecture, Android, iOS, Windows, macOS, Linux, cross-platform
- Implementation documentation: repository architecture, Rust core, Kotlin layer, Swift layer, TypeScript layer, Python layer, storage, observability, configuration
- Testing documentation: strategy, unit testing, integration, property testing, fuzzing, adversarial, interoperability, cross-platform, failure testing, regression
- Performance documentation: performance model, benchmarking, battery, bandwidth, latency, scale, performance budgets
- Operations documentation: observability, telemetry, logging, diagnostics, deployment, field operations, incident management
- Research documentation: methodology, technology landscape, competitive analysis, academic research, standards review, research gaps, open problems
- Legal documentation: legal research, India compliance, privacy requirements, telecom considerations, spectrum, satellite regulation, data governance, compliance risk register
- Product documentation: requirements, UX principles, emergency UX, accessibility, product roadmap
- Business documentation: business model, B2B, B2G, enterprise, SDK, hardware, go-to-market
- Decisions: ADR index, ADR-0001 through ADR-0006
- Requirements: REQUIREMENTS_INDEX.md with REQ-001 through REQ-020
- Experiments: EXPERIMENT_INDEX.md with EXP-001 through EXP-005
- Benchmarks: BENCHMARK_INDEX.md with BENCH-001 through BENCH-005

### Engineering Control Plane (`/engineering`)
- PROJECT_GRAPH.yaml: 32-node dependency graph with typed edges
- NODE_SCHEMA.yaml: complete node schema definition
- EDGE_SCHEMA.yaml: edge type definitions
- AGENT_REGISTRY.yaml: 20 specialized agents with missions, capabilities, constraints
- AGENT_CAPABILITIES.yaml: capability-to-agent mapping
- EXECUTION_POLICY.yaml: autonomy level 4, allowed/forbidden actions, approval gates
- RISK_POLICY.yaml: risk levels and thresholds
- APPROVAL_POLICY.yaml: high-risk gate definitions
- ACCEPTANCE_POLICY.yaml: node completion criteria
- RESEARCH_POLICY.yaml: evidence hierarchy and research methodology
- TEST_POLICY.yaml: testing requirements and coverage targets
- SECURITY_POLICY.yaml: security engineering requirements
- DOCUMENTATION_POLICY.yaml: documentation standards
- PRIORITY_POLICY.yaml: priority calculation formula
- LOOP_POLICY.yaml: node execution loop definition
- ESCALATION_POLICY.yaml: when and how to escalate
- AUTONOMY_POLICY.yaml: autonomy levels and boundaries
- PROJECT_STATE.yaml: current project state snapshot
- CHANGELOG.md: this file

### Key Decisions (in ADRs)
- ADR-0001: Rust as primary networking core (ACCEPTED)
- ADR-0002: CBOR as wire format (ACCEPTED)
- ADR-0003: Ed25519 + X25519 + ChaCha20-Poly1305 cryptographic suite (ACCEPTED, pending crypto review)
- ADR-0004: BLE as primary short-range discovery transport (ACCEPTED)
- ADR-0005: Monorepo architecture (ACCEPTED)
- ADR-0006: Opportunistic routing with PRoPHET + spray-and-wait (PROVISIONAL)

### Initial Project Graph
- 32 engineering nodes defined
- Critical path identified: VISION ΓåÆ REQ ΓåÆ ARCH ΓåÆ PROTO ΓåÆ MSG ΓåÆ ROUTE ΓåÆ SCF ΓåÆ EMERG ΓåÆ ANDROID ΓåÆ PILOT
- 5 high-risk nodes requiring human approval identified
- 5 research gaps identified with planned experiments
- 5 open legal questions requiring lawyer review

### Impact
- Autonomous engineering system has complete blueprint to begin implementation
- All policies defined for autonomous operation
- High-risk gates established
- Next recommended action: begin PROTO-001 (Protocol Design implementation)

> **Recovery note (2026-08-15, iter 58):** the uncommitted prose for CHANGELOG
> versions 0.3.0–0.3.4 was lost when the file was overwritten in-pass. Those
> releases are fully preserved, entry-for-entry, in
> `engineering/memory/records/execution-log.md` (iterations 33–56), which remains
> the authoritative per-iteration record. 0.3.5/0.3.6 below are verbatim.

## [0.3.5] - 2026-08-15 - SECURITY | BUG

**CRYPTO-001 IMPLEMENT COMPLETE - crypto stack, encryption wiring, and at-rest row seal landed (iter 57).**

### Added
- iris-storage at-rest seal: RowSealer trait + StorageKeySealer (ChaCha20-Poly1305, AAD = message_id||priority||expires_at, per-row random 12B nonce) + NoSealer default; PgStorage::with_sealer wired through persist/load/get_queue; schema v2 MIGRATE_DROP_PLAINTEXT_IDENTITY (sender_id/recipient_id columns + idx_messages_recipient dropped idempotently at connect).
- RFC 5869 App A.2 + A.3 KATs (authoritative vectors from rfc-editor.org) + sender/recipient same-key derivation unit test (AC-5).
- M7 AC-8 engine E2E suite (crates/iris-core/tests/crypto_e2e.rs): encrypted+signed round-trip over a shared SimulatedTransport + tampered-wire-frame rejection.
- STORAGE.md at-rest section rewritten (no longer deferred to the gate).

### Changed
- crypto kdf.rs doc comments de-fenced (pre-existing doctest break fixed); message_engine/mod.rs ok_or_else -> ok_or (clippy -D warnings clean).

### Fixed
- Pre-existing kdf.rs doctest failures (HKDF::new(...) pseudo-code blocks executed as doctests).
- Clippy unnecessary_lazy_evaluations diagnostic.

### Verified
- Workspace 284 green (233 core incl. 26 crypto + 2 M7 e2e + 8 obs + 8 sim + 4 + 10 storage + 13 pg_store incl. sealed-at-rest + 1 M3 + ML), clippy --all-targets -D warnings 0.

## [0.3.6] - 2026-08-15 - SECURITY

**CRYPTO-001 SECURITY_REVIEW disposition pass (iter 58) - redteam FAIL-with-condition resolved to FIXED/TRACKED.**

### Fixed
- **RED-0004 (HIG)** fragmentation broke E2EE. Now: distinct per-fragment wire ids (`derive_fragment_message_id`) so dedup no longer swallows fragment 2; fragment header carries `orig_payload_type` + the original whole-ADU signature; reassembly restores message_id/payload_type/payload_ref/signature; `process_incoming` re-verifies the reassembled envelope; `FragmentSet::bind_sender` rejects mixed-signer sets (RED-0007).
- Two latent routing bugs (found proving RED-0004): `TransportSelectionRequest::fragmentable` so oversized P4-P7 bulk is selectable on small-MTU/BLE transports, and the engine's bottleneck-MTU clamp (was `max_mtu = ::MAX` via `.max()`, so fragmentation never fired in the real delivery path).
- RED-0002 (HIG): missing-key unicast fail-open now emits `msg.sent_unencrypted` warning + `iris.messages.sent_unencrypted_total` metric; threat model documented in CRYPTO_DESIGN.md.
- RED-0006 (MED): weak-key forgery rejection test `verify_strict_rejects_weak_small_order_key_forgery`; KAT table corrected (§7.1 is a normal vector, not weak-key).
- RED-0009 (LOW): `crypto::ed25519::verify` -> `verify_strict` (name enforces the invariant).
- RED-0012 (INFO): schema.rs `payload_size` comment corrected.

### Added
- Engine-level M7 E2E `m7_fragmented_encrypted_message_reassembles_and_verifies` (real-path fragment proof: 70 KiB P5 unicast -> 2 wire fragments -> reassemble -> whole-ADU verify -> decrypt -> plaintext).
- `engineering/memory/records/CRYPTO-001_SECURITY_REVIEW.md` (findings RED-0001..0012, evidence, dispositions with ID scope clarification vs transport RED-0001/0002 records).

### Tracked (IDENT-001 / PROTO-001 v2)
- RED-0001 (CRITICAL): production desktop binary wiring (IrisCryptoProvider + persisted identity + key directory + sealer) - **operator gate**; CRYPTO-001 core (provider + engine + at-rest seal) complete + tested.
- RED-0003 (signed encryption-intent flag), RED-0005 (identity-key binding), RED-0008 (auth_cert_chain validation), RED-0010 (16-byte sender_id), RED-0011 (small-order hardening).

### Verified
- Workspace 290 green (from 284), clippy `-D warnings` 0.

## [0.3.7] - 2026-08-15 - SECURITY | NODE

**CRYPTO-001 VERIFY + ACCEPT (iter 59) - node COMPLETE; redteam RED-0001 operator-ratified; next node IDENT-001.**

### Added
- `docs/implementation/CRYPTO_VERIFICATION.md` v1.0: AC-1..9 all PASS (crypto module + crate pins; RFC 7748 §5.2/§6.1 KATs; RFC 8439 §2.8.2 KAT + tamper; RFC 8032 §7.1 + verify_strict weak-key reject + verify() not callable; RFC 5869 App A 1-3 + sender/recipient same-key; E2E seam round-trip + tampered wire; at-rest seal round-trip/tamper/dump-resistance/plaintext-index; engine M7 incl. fragmented-encrypted reassemble+verify; P0 broadcast not encrypted; no-FS honesty + no libcrux misattribution).
- `engineering/memory/records/DEC-0010.md`: operator ratification - RED-0001 (CRITICAL, inert DevCryptoProvider in iris-desktop) tracked to IDENT-001 with security control: **no production deployment with DevCryptoProvider / no identity / no sealer**.

### Changed
- PROJECT_GRAPH.yaml: CRYPTO-001 status **COMPLETE** (evidence + acceptance_criteria AC-1..9 + known_limitations RED-0001..0011/FS/opt-in seal); validation_status (18 COMPLETE); PROJECT_STATE reconciled (completed 18 / implementing 0 / designing 1); CRYPTO-001_SECURITY_REVIEW.md RED-0001 marked RATIFIED; ALLOCATION.md DEC-0010 registered.

### Next
- IDENT-001 (P0, deps CRYPTO-001 COMPLETE): identity/key mgmt/trust/address derivation; absorbs RED-0001 (desktop provider wiring + persisted identity + key dir + sealer), RED-0005, RED-0008.

### Verified
- Workspace 290 green, clippy `-D warnings` 0.
