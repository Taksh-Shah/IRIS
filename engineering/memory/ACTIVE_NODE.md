# ACTIVE_NODE.md

**Schema version**: 1.0
**Last updated**: 2026-08-15T22:05:00Z

---

## Node Identity

| Field | Value |
|-------|-------|
| Node ID | SEC-001 |
| Name | Security Hardening |
| Type | SECURITY |
| Priority | P0 |
| Status | IMPLEMENT (iter 74, STAGE_TRANSITION) |
| Risk level | HIGH (security) |
| Autonomy level | 5 (fully autonomous) |
| Requires human approval | No — gate RESOLVED 2026-08-14 (DEC-0009) |

## Description

Threat-model implementation, DoS resistance, replay protection, Sybil
mitigation. Fourth P0 SECURITY node (after CRYPTO-001, IDENT-001, EMERG-001 —
all COMPLETE). Depends on CRYPTO-001 (AEAD/E2E encryption + signatures),
MSG-001 (message engine), ROUTE-001 (baseline routing).
Selected per PRIORITY_POLICY (P0; deps all COMPLETE; lowest-index eligible after
EMERG-001). Quality process retained: UNDERSTAND → RESEARCH → REQUIREMENTS/
DESIGN → IMPLEMENT → TEST → SECURITY_REVIEW (redteam) → VERIFY → ACCEPT.

## Dependencies (must be COMPLETE)

- CRYPTO-001 ✅ COMPLETE (iter 59 — AEAD/E2E + at-rest seal; RED-0001..0011)
- MSG-001 ✅ COMPLETE (message engine, 94/94)
- ROUTE-001 ✅ COMPLETE (baseline routing; replay/propagation constraints)

## Inputs to SEC-001 (recorded known_limitations from completed nodes)

- **RED-0002** (CRYPTO-001): do-not-send-unencrypted is a mid-flight metric +
  warn; fail-closed enforcement deferred → SEC-001 operational policy (ACL-1).
- **RED-0003 / EMERG-RT-003 / ADR-0011 issue**: `encryption_intent` signed flag
  (send-side authenticated intent) deferred to PROTO-001 v2 / SEC-001.
- **EMERG-001 RT-003**: engine-ledger CANCEL resolution; **RT-010**: pre-gate
  emergency audit gap (counter observability).
- **REPLAY/SYBIL/DoS**: ROUTE-001 + EMERG RT-006 delivered bounded replay guards;
  DENM/dedup + SOS rate limit exist; SEC-001 generalizes to all message classes.
- ADR open items (ADR-0005/0006/0011) feed the SEC-001 design.

## UNDERSTAND (COMPLETE — iter 72)

Threat model synthesised from `docs/security/*`:
- **Adversary classes A–F** (ADVERSARY_MODEL.md): A passive eavesdropper;
  B active inject/replay; C honest-but-curious relay; D malicious relay
  (blackhole, false route advertisements); E compromised node; F quantum
  future (ML-KEM-512 / ML-DSA-44 v2.0 posture — not an IRIS requirement).
  Operational threat actors TA-1..9.
- **Explicit non-guarantees**: anonymity / traffic-analysis resistance
  (ADVERSARY_MODEL.md:213); availability under a malicious-relay adversary Adv_M
  (:224); strong Sybil resistance for the open mesh (THREAT_MODEL.md:172).

**AC check (confirmed)**: SEC-001 node **LACKS `acceptance_criteria`** in
PROJECT_GRAPH.yaml (lines 469-475 carry only description + dependencies) → C2
conflict (same pattern as EMERG-001): **REQUIREMENTS/DESIGN must define ACs.**
ACCEPTANCE_POLICY gating: SECURITY-type additional (threat-model section
updated, Security_Agent review, adversarial test scenarios, fuzzer if parsing,
no secrets in logs) **+ safety_critical_additional applies_to [SEC-001]**:
adversarial_test_coverage >95%, fuzzing_duration 24h minimum,
external_security_review recommended, human_approval true (DEC-0009 lifted the
gate; quality process retained).

**DISCOVERY DISC-0013 (verified by glob+grep — doc-vs-code drift)**:
`SYBIL_RESISTANCE.md` claims Defenses 3 (Rate Limiting), 4 (Anomaly Detection,
L2 Intelligence), 5 (Reputation System) as Implemented, but **no general
per-message-class rate limiter / reputation / Sybil detector exists in code**.
`DOS_RESISTANCE.md:213` claims all DoS defenses live in
`crates/iris-core/src/node/dos_protection.rs` — **that path does not exist**
(`crates/iris-core/src/node/` absent). **What IS implemented**:
message dedup Bloom + exact LRU (`message_engine/dedup.rs`), routing
`ForwardedCache` (`routing/dedup_cache.rs`), emergency-only `SosRateLimiter` +
`BroadcastReplayGuard` (`emergency/rate_limit.rs` + `provider.rs`), identity
`trust_store` replay counters (`rotate.rs` earliest-seen-wins).
→ SEC-001 design must **generalize DoS/replay/Sybil/spam across all message
classes** and **reconcile claimed-vs-implemented docs** (C-pattern doc
corrections, as EMERG-001).
security docs (13 files), decisions/ADR-0002..0011, ORCH-0001 SEC-001
work-package, accepted-node known_limitations (inputs table above).

## RESEARCH (COMPLETE — iter 73)

**RES-0018 recorded + registered in ALLOCATION.md (next RES-0019)** — DoS /
replay / Sybil / spam 2024-2026 SOTA for DTN mesh, 12 evidence passes,
evidence-leveled (RFC/NIST L1, academic L2, IETF-draft/tech-report L3,
OSS/internal L4; no AI citations; "BBPATs" unresolvable → Gap G1).
- **DoS (Q1)**: ADOPT srTCM token bucket per (sender,class), silent-drop,
  P0/P1 drop-exempt (RFC 2697/2698/4115; RFC 9171 §6.9 CL MUST rate-limit);
  ADOPT per-sender storage quota + priority-reserved pool + TTL-ordered
  eviction (Claim-Carry-and-Check); ADOPT Meshtastic CVE regression suite
  (CVE-2024-47065 traceroute 2:1 reflected DoS, CVE-2025-21608/24798
  want_response crash, CVE-2025-24797 parser, CVE-2025-55293 amplification,
  CVE-2025-52464 cloned keys, CVE-2026-42566 NodeID forge); **REJECT
  per-message PoW** (battery/duty-cycle/difficulty-calibration — Hristozov
  AISEC, Springer MTU analysis); DEFER identity-mint PoW (KeyChallenge/SyDeLP)
  + VDF + RFC 7859 to PROTO-001 v2 — **no new crypto crates (constraint
  held)**.
- **Replay (Q2)**: layered = exact dedup + freshness window (per-source skew)
  + per-sender high-water (ts,seq) (RFC 9171 §4.2.7 lexica, RFC 4303 §3.4.3
  window concept, RFC 7181 §23.2/RFC 7183 HMAC-SHA256+timestamp
  DTN-time-substituted, RFC 6479) + persistence; **dedup Bloom+LRU persistence
  promoted WP-2 → SEC-001** (cross-reboot hole, `message_engine/mod.rs:24`);
  message-id collision bounded by NIST SP 800-107 (~2^128 second-preimage,
  self-collision harmless); fragment reassembly re-enters dedup/freshness.
- **Sybil (Q3)**: per-identity limits necessary-not-sufficient (Douceur IPTPS
  2002); REJECT SybilGuard/SybilLimit/global reputation on relays (connected
  fast-mixing graph assumption fails sparse DTN); ADOPT Ostra NSDI 2008
  link-credit budgets (budget = f(trusted links)) + Briar BQP QR
  verified-pairing as offline identity-cost primitive (OTF-005 unverified-tier
  annotation); key-hygiene CVEs → IDENT-001 regression tests.
- **Spam (Q4)**: ADOPT reputation = local watchdog + verified second-hand
  positive-only (CORE) + iTrust audits as **routing weight only, never an
  admission gate** (Watchdog/Pathrater, iTrust TPDS 2014, ITRM, Rep-AODV 2024);
  ADOPT receiver/UI-side Bayesian spam scoring only (no relay-side content
  drops — FP on emergency intolerable); ADOPT RED-0002 ACL-1 key-anchored
  per-class sender allowlist over existing `emergency/authority.rs`
  (CAP v1.2 DSig, ETSI TS 102 900 §5.5 no-UE-auth → CACM 2023 WEA Ed25519).
- **Routing attacks (Q5)**: blackhole = replication + Ack-driven eviction
  (Epidemic Oracle ISCC 2025) + reputation weight; REJECT RTT/location wormhole
  detectors (EAODV-ARP needs timely RTT) + GPS secure-position (no-GPS
  posture); DEFER SAND signed neighbor discovery to PROTO-001 v2.
- **Q6**: CONFLICTS C1–C6 (DISC-0013 doc-vs-code drift) feed DESIGN C-pattern
  doc reconciliation; gaps G1–G7 (BBPATs → RFC 9171/9172/9713, freshness τ →
  EXP-SEC-001, snapshot sizing → benchmark).
Workspace untouched (research-only).
**Next: DESIGN** — `docs/implementation/SEC_001_DESIGN.md` + **define ACs**
(mandatory, C2) per ACCEPTANCE_POLICY safety_critical_additional (>95%
adversarial coverage, 24h fuzzing, external review recommended) +
DEC-SEC-0001..N formalization → IMPLEMENT → TEST → SECURITY_REVIEW → VERIFY →
ACCEPT.

## DESIGN (COMPLETE — iter 74)

**`docs/implementation/SEC_001_DESIGN.md` v1.0** — general per-message-class
security hardening across P0-P7 extending EMERG-001 defenses, **zero new wire
fields + zero new crypto crates** (D6, RES-0018 R11). **Module layout**:
`security/{rate_limiter,quota,replay,reputation,spam,acl}` behind a
`SecurityPolicy` facade with `Noop` default (AC-12: un-armed engine
unchanged). **AC-1..AC-16 defined** in PROJECT_GRAPH.yaml (resolves the C2
gap). **DEC-SEC-0001..0008 ratified** in DECISIONS.md: token bucket (RFC 2697),
storage quota + priority pool, cross-reboot replay persistence, reputation
scope (routing-weight-only), receiver-side spam scoring, ACL-1 emergency
authorization, PoW REJECT, SybilGuard/Limit REJECT. Absorbs RED-0002 (ACL-1),
RED-0003/EMERG-RT-003/ADR-0011 (signed encryption_intent), EMERG-RT-010.
Reconciles DISC-0013 doc-vs-code drift via doc-reconciliation matrix (§6 →
AC-13). Boundary: PoW REJECT (DEC-SEC-0007), SybilGuard/SybilLimit REJECT
(DEC-SEC-0008), no new crypto (R11).

## IMPLEMENT (in progress — iter 74)

Build `security/` module per SEC_001_DESIGN.md: rate_limiter.rs (srTCM RFC 2697
per (sender_short[16B],class), lazy refill, silent-drop, P0/P1 drop-exempt,
unknown-sender aggregate bucket), quota.rs (per-sender quota + priority-
reserved pool, P0/P1 never evicted by quota, TTL/lifetime order), replay.rs
(freshness window per-source skew + per-sender high-water (ts,seq) + dedup
Bloom+LRU snapshot + high-water persistence batched crash-safe), reputation.rs
(bounded per-peer Bayesian score, routing-weight-only, CORE positives,
iTrust-style audits), spam.rs (receiver-side likely_spam annotation only,
P0-P3 disjoint), acl.rs (ACL-1 per-class key/role allowlists reusing
emergency/authority.rs verify). Wire SecurityPolicy facade + Noop default into
MessageEngine ingest + deliver_or_relay. Then TEST (AC-1..16 evidence incl.
cross-reboot replay, adversarial >=95%, fuzz) → SECURITY_REVIEW (redteam
SEC-RT) → VERIFY → ACCEPT.

## Node completion gate

ACCEPT requires: threat model implemented (replay protection, DoS resistance,
Sybil mitigation, spam resistance operationalized across the engine), ADR
open items resolved or explicitly deferred, doc-vs-code drift reconciled
(C-pattern: SYBIL_RESISTANCE / DOS_RESISTANCE claimed defenses either
implemented or corrected), abuse/governance threat review + redteam + verifier
evidence + workspace green. CRITICAL findings halt for operator report
(RISK_POLICY). ACCEPTANCE_POLICY safety_critical_additional for SEC-001:
adversarial coverage >95%, 24h fuzzing minimum, external review recommended.

## Prior Active Node (for continuity)

**EMERG-001 (P0 SECURITY/safety) — COMPLETE (iter 71).** Emergency System ACCEPTED:
EMERG_VERIFICATION.md v1.0 AC-1..14 PASS; independent verifier APPROVE
(reproduced 431 passed/0 failed, clippy 0, cargo audit 0 vulns, all 6 redteam
fixes present); redteam EMERG-RT-001..011 PASS-with-recorded-deviations
(RT-001 CRITICAL authority-root + RT-002 HIGH SOS enforcement FIXED); workspace
431 green. 20 COMPLETE nodes.

**IDENT-001 (P0 SECURITY) — COMPLETE (iter 63c/64).** Identity System ACCEPTED:
IDENT_VERIFICATION.md v1.0 AC-1..11 all PASS (verifier APPROVE); 361 green.

See NEXT_ACTION.md / CURRENT_STATE.md.