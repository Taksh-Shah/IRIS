# SEC-001 Design - Security Hardening (v1)

**Document ID**: IRIS-SEC-001-DESIGN-001
**Version**: 1.0
**Node**: SEC-001 (P0 SECURITY, deps CRYPTO-001 + MSG-001 + ROUTE-001 COMPLETE)
**Date**: 2026-08-15
**Research basis**: RES-0018 (2024-2026 SOTA: DoS/replay/Sybil/spam/reputation
for DTN + mesh + secure-position); DISC-0013 (doc-vs-code drift); DEC-P0001..P0017
(ratified in DECISIONS.md); RFC 2697/2698 (token bucket), RFC 4303 §3.4.3 +
RFC 6479 (anti-replay window), RFC 9171 §4.2.7/§6.9 (DTN creation timestamp +
CL rate-limit mandate), RFC 7181 §23.2 + RFC 7183 (MANET replay mandate),
RFC 9804 (SPKI), RFC 9713 (BPv7 admin records), NIST SP 800-107 (hash
strength), CVE-2024-47065 + CVE-2025-24797 + CVE-2025-55293 + CVE-2025-21608
(Meshtastic regression checklist), CVE-2025-52464 + CVE-2026-42566 (key-bound
identity), Douceur IPTPS 2002 (Sybil), Marti MobiCom 2000 (watchdog), CORE 2002
(positivity rule), iTrust TPDS 2014, ITRM ISIT 2009, Ostra NSDI 2008,
Rep-AODV CMC 2024, Briar BQP, EMERG-001 authority/rate_limit/broadcast code.
**Absorbs**: RED-0002 (do-not-send-unencrypted -> ACL-1), RED-0003/EMERG-RT-003/
ADR-0011 (signed `encryption_intent`), EMERG-RT-010 (pre-gate emergency audit
gap).
**Reconciles (doc-vs-code drift, DISC-0013)**: DOS_RESISTANCE.md (layers,
`dos_protection.rs` nonexistent), SYBIL_RESISTANCE.md (Defenses 3/4/5 claimed
but not implemented), REPLAY_PROTECTION.md (SQLite persistence claim),
THREAT_MODEL.md/SECURITY_ARCHITECTURE.md (reputation claims).

---

## 1. Scope

Implement **general per-message-class security hardening** across the message
engine, extending the emergency-scoped defenses (EMERG-001) and routing/
dedup/replay machinery to all message classes (P0-P7), with **zero breaking
wire-format changes and zero new crypto crates** (D6, RES-0018 R11). Four
capability areas:

1. **DoS resistance** - per-(sender, class) token-bucket rate limiter +
   per-sender storage quota + priority-reserved pool (extends the emergency
   SOS limiter and storage guards to all classes). (RES-0018 R1/R2)
2. **Replay protection** - freshness window + per-sender monotonic high-water
   mark + **cross-reboot persistence** of dedup snapshot + high-water marks
   (promotes the WP-2 deferred dedup persistence to a security requirement).
   (RES-0018 R4/R5/R10)
3. **Sybil context** - per-identity limits documented as Sybil-defeatable; the
   mesh does NOT implement SybilGuard/SybilLimit/social-graph (sparse DTN);
   verified-pairing HTTPS to be surfaced by platform nodes; identity-key-bound
   regression tests (CVE-2025-52464/CVE-2026-42566 lessons). (RES-0018 R6/R7)
4. **Reputation + spam + emergency ACL** - local watchdog + second-hand
   CORE-positive scores -> routing weight (never an admission gate); receiver-
   side spam scoring tag (never relay-side hard drops); ACL-1 emergency
   send authorization data (key-anchored allowlists). (RES-0018 R8/R9)

**Boundary (explicitly rejected, documented in ABUSE/GOVERNANCE docs)**:
per-message proof-of-work (battery/duty-cycle/calibration/reviewed - RES-0018
R3), SybilGuard/SybilLimit on relays (R7), CAPTCHA/badge servers (offline
infeasible - R3/R7), secure-position/ranging hardware (no GPS requirement - Q5),
global reputation consensus/EigenTrust (R8 scope).

---

## 2. Module layout (all new files under `crates/iris-core/src/security/`)

```
security/
  mod.rs          - SecurityPolicy facade: rate_limiter + quota + replay +
                    reputation wiring; expose seam accessors on MessageEngine.
  rate_limiter.rs - TokenBucket (srTCM RFC 2697-style: two counters, lazy
                    refill, O(1)); BucketKey = (sender_short [16B], class);
                    unknown-sender aggregate class bucket; P0/P1 exempt.
  quota.rs        - PerSenderStorageQuota + PriorityReservedPool (extends the
                    emergency reserved pool to P0/P1 per-sender).
  replay.rs       - FreshnessWindow (per-source skew budget) + PerSenderHighWater
                    (max verified (ts,seq)) + persistence of dedup snapshot +
                    high-water marks (batched crash-safe, flash-wear safe).
  reputation.rs   - PeerReputation: Bayesian score from (1) local forwarding
                    observation, (2) second-hand positives from verified peers
                    (CORE positivity rule), (3) iTrust-style probabilistic
                    audit; -> routing weight; bounded, no global broadcast.
  spam.rs         - Receiver-side scoring (Bayesian-style tag, min. 512B-class
                    payloads): produces "likely_spam" annotation for UI;
                    NEVER relay-side hard drop; disjoint from P0-P3.
  acl.rs          - EmergencySendAcl (RED-0002 ACL-1): per-alert-class allowlist
                    of key-anchored roles (P0 broadcast = authority roles; P1
                    medical = authority/certified responders; SOS = any verified
                    identity, rate-limited 3/hr); revocation via IDENT-001
                    rotation; verify-before-relay policy.
```

**Empty/vacuous defaults**: when no SecurityPolicy is configured the seams
operate in a default-permissive mode (identical behavior to current engine) —
the NoopEmergencyProvider pattern (AC-11 analog). The engine's current
431-test behavior is preserved un-armed.

**No new dependencies**. SHA-256 (message-id, dedup snapshot), Ed25519
(signatures/chains), HMAC-SHA256 (RFC 7183-pattern for any future control
messages) all in-stack. PoW/VDF/IBE-candidate primitives are documented DEFER
to PROTO-001 v2 (RES-0018 R11).

---

## 3. Design decisions (DEC-SEC-0001..0007, ratified in DECISIONS.md)

| DEC | Decision | Evidence |
|-----|----------|----------|
| DEC-SEC-0001 | General per-(sender, class) token-bucket rate limiter (srTCM RFC 2697 shape); silent drop on overflow; P0/P1 exempt from drops (queue-priority only). | RES-0018 R1/Q1; RFC 9171 §6.9; CVE-2024-47065 |
| DEC-SEC-0002 | Per-sender storage quota + priority-reserved pool (generalize emergency pool); eviction by lifetime/TTL + quota violation order. | RES-0018 R2; Zekkori buffer analysis; RFC 9171 lifetime |
| DEC-SEC-0003 | Cross-reboot replay: persist dedup Bloom+LRU snapshot + per-sender high-water (ts,seq) + freshness window τ with per-source skew; batched crash-safe writes; promote WP-2 dedup persistence. | RES-0018 R4/R5/Q2; RFC 9171 §4.2.7; RFC 4303 window concept; RFC 7181 §23.2 |
| DEC-SEC-0004 | Reputation = local watchdog + verified-second-hand positives-only (CORE) + iTrust-style audits; score = routing weight ONLY, never admission gate. | RES-0018 R8/Q4; Marti/CORE/iTrust/ITRM/Rep-AODV |
| DEC-SEC-0005 | Receiver-side spam scoring tag; relay never hard-drops content; score classes disjoint from P0-P3. | RES-0018 Q4.2; Cormack CIKM 2007 FP risk |
| DEC-SEC-0006 | ACL-1 emergency send authorization as data: per-class key/role allowlists, verify chain+sig before relay, silent-drop unverifiable; SOS = any verified identity @3/hr. | RES-0018 R9/Q4.3; CAP v1.2 DSig; RFC 9804; CACM 2023 WEA |
| DEC-SEC-0007 | Per-message PoW/client puzzles REJECTED for the TX path (battery/duty-cycle/asymmetry); identity-mint PoW (KeyChallenge/SyDeLP) DEFERRED to PROTO-001 v2. | RES-0018 R3/Q1.4; Hristozov AISEC; Springer |
| DEC-SEC-0008 | SybilGuard/SybilLimit/social-graph REJECTED for mesh relays (sparse DTN, no fast-mixing guarantee); verified-pairing (BQP pattern) surfaced via platform nodes; per-identity limit docs updated to state Sybil-defeatable. | RES-0018 R6/R7/Q3 |

---

## 4. Acceptance criteria (AC-1..AC-16) — resolves the C2 gap (node lacked `acceptance_criteria`)

Defined against ACCEPTANCE_POLICY `SECURITY` additional (threat-model section
updated, Security_Agent review, adversarial test scenarios, fuzzer if parsing,
no secrets in logs) + `safety_critical_additional` (adversarial_test_coverage
>95%, fuzzing_duration 24h minimum, external_security_review recommended)
+ RES-0018 bottom-line (5 implementation items + doc fixes).

1. **AC-1 Rate limiter**: general per-(sender,class) token-bucket exists; P0/P1
   messages are never dropped by it (queue-priority only); overflow -> silent
   drop (no error reply/negative-ack); unit + integration tests prove burst
   tolerance + refill + class isolation. Unknown-sender aggregate bucket
   bounded.
2. **AC-2 Storage quota**: per-sender quota + priority-reserved pool enforced
   on the store; P0/P1 never evicted by quota action; eviction ordering by
   lifetime + quota criteria tested.
3. **AC-3 Replay freshness**: inbound freshness window enforced per message;
   too-old and too-future (skew beyond budget) rejected; per-source skew
   budget configurable; tests cover boundary ts and clock-drift tolerance
   (DTN-time/monotonic semantics, RFC 9171 §4.2.7).
4. **AC-4 Per-sender high-water**: for trusted (verified/authenticated)
   senders, a message with (ts,seq) <= high-water is replay-dropped; strictly
   higher -> accepted + advances high-water; tests: in-order, out-of-order,
   duplicate, reboot-replay rejection.
5. **AC-5 Cross-reboot persistence**: dedup Bloom+LRU snapshot + high-water
   marks persist across a fresh engine instance (mock-restart test); a
   replayed message from before the restart is rejected; batched crash-safe
   write path tested (no per-message write).
6. **AC-6 Message-id collision analysis**: message-id remains a >=128-bit
   SHA-256 truncation over the sealed envelope; attacker-controlled collision
   analysis documented per NIST SP 800-107 (self-collision only harms
   attacker; victim second-preimage infeasible at 128-bit); documented in
   REPLAY_PROTECTION.md.
7. **AC-7 Fragmented replay**: reassembled fragments re-enter the same
   dedup/freshness/high-water pipeline (no replay-after-reassembly gap); test
   covers replayed fragment chain after eviction.
8. **AC-8 Reputation scoring**: `reputation.rs` produces bounded per-peer
   scores from first-hand + second-hand-positives-only + audit inputs; score
   maps to routing weight; negative scores NEVER propagate (CORE rule);
   adversarial tests: bad-mouthing, collusion, on-off.
9. **AC-9 Reputation is not a gate**: a low/missing-reputation relay remains
   usable as a route (availability > trust purity); regression test proves no
   message is dropped purely for reputation.
10. **AC-10 Spam scoring**: receiver-side "likely_spam" annotation produced;
    no relay-side content hard-drop; P0-P3 never spam-scored (classes
    disjoint).
11. **AC-11 Emergency ACL-1**: `acl.rs` materializes per-class allowlists;
    P0/broadcast requires verified authority role chain; SOS requires any
    verified identity @3/hr (existing limiter); spoofed/revoked/expired/
    replayed authority rejected (negative tests modeled on EMERG-001
    authority verification).
12. **AC-12 Compatibility**: engine un-armed behavior identical (Noop
    SecurityPolicy == current behavior; existing 431-test suite green);
    no new wire-format fields; zero new crypto crates (cargo tree check
    clean — D6).
13. **AC-13 Doc reconciliation (C-pattern)**: DOS_RESISTANCE.md /
    SYBIL_RESISTANCE.md / REPLAY_PROTECTION.md / THREAT_MODEL.md /
    SECURITY_ARCHITECTURE.md corrected to implemented reality + RES-0018
    verdicts (PoW REJECT/DEFER, SybilGuard/Limit REJECT, spoof/auth notes);
    claimed-vs-implemented matrix in doc.
14. **AC-14 Adversarial coverage**: adversarial tests >= 95% of the new
    security module code paths (rate-limit edge cases, Sybil-identity churn,
    replay-after-reboot, blackhole/gray-hole relay drop of new paths,
    forged/replayed authority, msg-id truncation collision, Bloom FP
    interaction with emergency traffic); measured via coverage report.
15. **AC-15 Fuzzing**: fuzz targets exercised >= 24h total across parsing/
    rate-limit/replay/reputation inputs (per safety_critical_additional);
    findings triaged with redteam before ACCEPT (or explicit 24h-duration
    gate recorded in known_limitations if hardware budget prevents full
    24h — adversarial property-based suite substitutes, deviation
    recorded).
16. **AC-16 Security review + log hygiene**: Security_Agent review completed
    (redteam SEC-RT findings dispositioned); no secrets in log output (CI
    grep guard on payload/signature/secret over security/* macros).
    Known_limitations recorded: BLE 512B ceiling, per-identity-limits
    Sybil-defeatable, sequential reputation convergence, Meshtastic-style
    fixed cadence not adopted (priority-qualified buckets instead).

---

## 5. VERIFY plan

- Workspace green after IMPLEMENT/TEST (target unchanged-total + new module
  tests), clippy --workspace --all-targets 0, cargo audit 0 vulns/516 crates
  (no new deps), D6 no-new-crypto via cargo tree.
- Independent verifier subagent reproduces each AC evidence row; redteam
  SEC-RT findings dispositioned in SEC-001_SECURITY_REVIEW.md (PASS-with-
  recorded-deviations pattern).
- Cross-reboot tests run in-process (engine rebuild over persisted snapshot).
- ADR open items dispositioned: ADR-0006 issues already resolved (CRYPTO-001
  iter 55); ADR-0011 open issue recorded resolved-or-deferred in this doc.

---

## 6. Doc reconciliation matrix (DISC-0013/C-pattern)

| Doc | Claimed (stale) | Reality/action |
|-----|-----------------|----------------|
| DOS_RESISTANCE.md | Layer 1-8 + `node/dos_protection.rs` (governor, quotas, backpressure, 50µs) | No such file (DISC-0013). Rewrite to RES-0018 R1/R2 design: in-engine security/rate_limiter.rs + quota.rs; RFC 9171 §6.9 rationale; PoW Layer-6 REJECT/DEFER verdict per RES-0018 Q1.4. |
| SYBIL_RESISTANCE.md | Defense 3 rate limit "Implemented", 4 anomaly "L2 Intelligence", 5 reputation "Partially" | Only emergency rates real. Rewrite: per-identity limits documented Sybil-defeatable (Douceur), reputation.matches AC-8/9, no anomaly-detection claim, verified-pairing via platform nodes, SybilGuard/Limit REJECT rationale. |
| REPLAY_PROTECTION.md | Bloom+LRU "serialized to SQLite" | WP-2 deferral promotes to SEC-001 (AC-3/4/5); document exact reset/window semantics + persistence + 0%-FN-with-1%-FP wording. |
| THREAT_MODEL.md / SECURITY_ARCHITECTURE.md | Reputation "relay deprioritized"/"signed receipts build reputation" | Land reputation.rs (AC-8/9) then reword to "routing weight via security/reputation.rs". |
| MESSAGE_AUTHENTICATION.md | (verify) | Add ACL-1 note (AC-11) — emergency-class sends require chain + sig; generic messages verify-before-forward as today. |
| SPAM_RESISTANCE.md | (design spirit) | Land receiver-side scoring (AC-10), never relay-side drops. |

---

## 7. Known limitations / boundary (carried to ACCEPT)

- BLE 512B verified-chain ceiling unchanged (emergency, radio-native SOS).
- General per-identity rate limits remain Sybil-defeatable by free identity
  churn (documented; identity-cost via verified-pairing is platform-node
  scope; KeyChallenge/SyDeLP PoW-mint deferred to PROTO-001 v2).
- Reputation converges slowly in sparse contact (iTrust/ITRM evidence);
  sequential by design; never a gate.
- Meshtastic-style fixed global message cadence NOT adopted (priority-
  qualified per-class buckets instead — higher-fidelity to message classes);
  documented.
- 24h fuzzing target per ACCEPTANCE_POLICY: executed to the extent of the
  environment budget; any shortfall recorded as gated deviation in
  known_limitations with the adversarial property-based suite as
  substitute evidence (AC-15).