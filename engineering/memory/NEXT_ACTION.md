# NEXT_ACTION.md

**Schema version**: 1.0
**Last updated**: 2026-08-15T22:05:00Z

---

## Priority: SEC-001 IMPLEMENT — DESIGN COMPLETE (iter 74), deps CRYPTO-001 + MSG-001 + ROUTE-001 COMPLETE

## Completed

**✅ SEC-001 DESIGN COMPLETE (iter 74) — STAGE_TRANSITION to IMPLEMENT.**

- **`docs/implementation/SEC_001_DESIGN.md` v1.0** — general per-message-class
  security hardening across P0-P7 (extends EMERG-001 defenses); **zero new
  wire fields + zero new crypto crates** (D6, RES-0018 R11). Module layout
  `security/{rate_limiter,quota,replay,reputation,spam,acl}` behind a
  `SecurityPolicy` facade with `Noop` default.
- **AC-1..AC-16 defined** in PROJECT_GRAPH.yaml SEC-001 node — resolves the C2
  gap (node entered with NO `acceptance_criteria`). ACs 1-7 = rate/quota/
  freshness/high-water/persistence/collision/fragment-replay; 8-9 = reputation
  routing-weight-only + never-a-gate; 10 = spam annotation; 11 = ACL-1;
  12 = Noop-compatibility; 13 = doc reconciliation (DISC-0013 C-pattern);
  14-16 = adversarial >=95%, fuzz >=24h, Security_Agent review + log hygiene.
- **DEC-SEC-0001..0008 ratified** in DECISIONS.md: token bucket (RFC 2697),
  storage quota + priority pool, cross-reboot replay persistence, reputation
  scope, receiver-side spam scoring, ACL-1 emergency authorization, PoW
  REJECT, SybilGuard/Limit REJECT.
- Absorbs RED-0002 (ACL-1), RED-0003/EMERG-RT-003/ADR-0011 (signed
  encryption_intent), EMERG-RT-010. Reconciles DISC-0013 via doc-reconciliation
  matrix. PROJECT_GRAPH SEC-001 DESIGNING + 16 ACs + clean stage_note.

**Prior:** RESEARCH COMPLETE (iter 73, RES-0018), UNDERSTAND COMPLETE (iter 72).

## Next Action

**SEC-001 IMPLEMENT (DESIGN COMPLETE → IMPLEMENT):**
1. Build `crates/iris-core/src/security/` per SEC_001_DESIGN.md:
   - `rate_limiter.rs` — srTCM RFC 2697 token bucket per (sender_short[16B],
     class), lazy refill, silent-drop, P0/P1 drop-exempt, unknown-sender
     aggregate bucket.
   - `quota.rs` — per-sender storage quota + priority-reserved pool, P0/P1
     never evicted by quota, lifetime/TTL ordering.
   - `replay.rs` — freshness window (per-source skew, DTN-time/monotonic) +
     per-sender high-water (ts,seq) + dedup Bloom+LRU snapshot + high-water
     persistence (batched crash-safe).
   - `reputation.rs` — bounded per-peer Bayesian score (watchdog +
     verified-second-hand positives-only CORE + iTrust audits), routing
     weight only.
   - `spam.rs` — receiver-side `likely_spam` annotation, never relay hard-drop,
     P0-P3 disjoint.
   - `acl.rs` — ACL-1 per-class key/role allowlists reusing
     emergency/authority.rs verify pattern.
   - `SecurityPolicy` facade + `Noop` default; wire into MessageEngine ingest
     + deliver_or_relay.
2. Then **TEST → SECURITY_REVIEW (redteam SEC-RT) → VERIFY → ACCEPT**; after
   ACCEPT → transports (BLE-001/WIFIAWARE-001/WIFIDIRECT-001/LORA-001/
   SAT-001) → TEST-001 → platform apps.

## Context

- **20 COMPLETE nodes**; SEC-001 (P0 SECURITY) active — fourth P0 SECURITY
  node; deps CRYPTO-001 + MSG-001 + ROUTE-001 COMPLETE.
- UNDERSTAND (72) + RESEARCH (73) + DESIGN (74) complete; 431-green workspace
  baseline to preserve through IMPLEMENT.

## Dependencies (must be COMPLETE)

- SEC-001 deps: CRYPTO-001 ✅, MSG-001 ✅, ROUTE-001 ✅.
- After SEC-001: transports (BLE-001/WIFIAWARE-001/WIFIDIRECT-001/LORA-001/
  SAT-001, BLK-0005 device tests gated) → TEST-001 → ANDROID-001/IOS-001/
  PILOT-001.

## State Transition After Completion

- SEC-001: UNDERSTAND ✅ (72) → RESEARCH ✅ (73) → DESIGN ✅ (74) →
  **IMPLEMENT (next)** → TEST → SECURITY_REVIEW → VERIFY → ACCEPT.

## Related Records

- SEC-001: SEC_001_DESIGN.md v1.0 ✅ · DEC-SEC-0001..0008 ✅ · AC-1..16 ✅ ·
  RES-0018 ✅ · DISC-0013 · conflicts C1-C6 · RED-0002 ACL-1, RED-0003/
  ADR-0011, EMERG-RT-010 inputs.
- Completed: RES-0017 · RES-0016 · RES-0015 · CRYPTO_VERIFICATION.md ·
  IDENT_VERIFICATION.md v1.0 · EMERG_VERIFICATION.md v1.0 · CHANGELOG 0.3.18.