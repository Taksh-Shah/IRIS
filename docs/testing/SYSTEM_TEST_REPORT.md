# IRIS System Test Report — Full Session Summary

**Document ID**: IRIS-SYSVAL-001-REPORT-001
**Date**: 2026-08-22
**Scope**: All tests executed during LORA-001 + SAT-001 + SYSVAL-001 pipeline
(iters ~160–178), plus re-verification of all pre-existing suites.
**Final baseline**: **727 passed / 0 failed / 1 ignored** across the full
workspace (`--all-features`).

---

## 1. New transports delivered this session

### 1.1 LORA-001 — LoRa Gateway Transport

**Regulatory compliance (WPC G.S.R. 853(E) 2021)**

The DutyCycleTracker enforces India's license-exempt SBD rules as a hard
legal floor: sliding 1-hour window, 36,000 ms airtime budget per device-hour,
with priority-proportional sub-caps (Emergency 60%, Medical 25%, Location
10%, Bulk 5%). There is NO runtime override knob — the only constructor path
validates against Table-I limits and refuses anything exceeding them.
Injections use an injectable millisecond clock so all timing math is
deterministic in tests.

**Radio physics**

Airtime is computed per the Semtech AN1200.13 formula as a pure function:
`(preamble + 4.25 + payload_symbols) × symbol_time`, with low-data-rate
optimization active for SF11+/BW125. Verified anchors: a full 255-B frame
costs 1,251 ms at SF9/BW125/CR4/5; the P0 profile uses SF12 (9,020 ms per
frame) trading airtime for ≈7 dB additional link budget.

**Carrier framing**

LoRaFrame v1 wraps envelopes in an 18-byte header `[version][priority]
[msg_id 16B]` inside the 340-B on-air cap — exactly fitting the design-pinned
237-B P0 emergency envelope. SLIP (RFC 1055) encoding handles the USB-serial
carrier with defensive parse: leading-END tolerance, escape validation,
4096-B allocation cap, zero panic on hostile input.

**Security review findings fixed**

The original implementation billed duty on PAYLOAD bytes only — the 18-B
header was free, enabling a minimal-frame flood to exceed the legal 1% duty
by up to 78%. Fixed to bill the FULL encoded frame. Additional fixes:
monotonic clock clamp (backward wall-clock steps ignored), exact-token
refunds on TX failure (day-aware decrement), malformed inbound frames
skipped-and-counted instead of aborting the poll batch, backlog depth cap
(256), and hot-plug lifecycle hardening.

### 1.2 SAT-001 — Satellite Gateway Transport

**Cost guard**

Satellite's constraint is money, not spectrum. The SatelliteCostGuard is
SOS-exempt: P0 messages are NEVER blocked regardless of remaining budget
(industry pattern per Zoleo/Garmin). P1/P2 face an hourly message cap
(default 10/hour) and a daily INR budget (default ₹200/day), each with
hard-stop + queue-not-drop semantics. Counters persist via a CostLedger
trait seam (in-memory oracle + Fanout multi-sink double-write harness).
Mailbox checks bill even when empty (~$0.05/poll) and count toward daily
spend.

**Hostile-pipe security**

The Iridium L-band link provides ZERO usable authentication or
confidentiality (proven by arXiv:2603.12062): SIM keys extractable, signaling
unencrypted, downlink spoofing demonstrated, replay succeeds, ~1 mW jamming
viable. Therefore: envelope verification is MANDATORY above the transport;
Ring Alerts are wake-up hints only; inbound attribution uses the zero-PeerId
fallback; satellite is best-effort redundancy, never guaranteed delivery.

**Security review findings fixed**

Monotonic clock clamp ported from LoRa; exact-token refunds replacing blind
newest-pop (preventing mispaired refunds under concurrency);
confirmation-hook panic contained via catch_unwind; MT-relay caps advertised
honestly (270 B not 340 B); hot-plug TOCTOU set closed; ledger bounded at
4096 events.

### 1.3 SYSVAL-001 — Whole-System Validation

Seven integration tests proving the assembled fabric behaves as ONE system:

| Track | Scenario | Proves |
|---|---|---|
| **A** | Three-node relay N1→[LoRa]→N2→[Sat]→N3; hot-plug chaos 8 cycles | Envelopes arrive byte-for-byte across heterogeneous links; chaos doesn't corrupt state |
| **B** | Four-leg DTN chain N1→N2→N3→N4 | Priorities preserved end-to-end; no loops; SCF semantics work |
| **D** | 1200-message abuse stream through rate-limiter+spam+reputation together | Burst semantics hold; every message accounted once; reputation bounded [0,1] and never gates; spam annotates never drops |
| **E** | P0 multipath + P3+-never gate | Satellite included for P0/P1 multipath only; P3–P7 hard-rejected transport-side |
| **F** | Backward clock-step injection into BOTH guards simultaneously | Budgets preserved (monotonic clamps hold); forward recovery legitimate |

Plus Phase 2 gates: workspace 727/0/1, clippy `-D warnings` = 0, fmt clean,
cargo audit exit 0.

---

## 2. Pre-existing suites (re-verified green every pass)

Every new transport change was validated against the ENTIRE existing test
corpus. The following subsystems were confirmed unaffected:

| Subsystem | Tests | Status |
|---|---|---|
| Crypto (AEAD, Ed25519, HKDF, small-order) | 20+ | ✅ Green |
| Crypto E2E (sign→encrypt→fragment→reassemble→verify→decrypt) | 3 | ✅ Green |
| Identity (PeerId derivation, provision, rotation, trust store) | 30+ | ✅ Green |
| Emergency (SOS codec, broadcast, authority chain, drill mode, rate limit) | 35+ | ✅ Green |
| Routing (epidemic flood, PROPHET, SCF, opportunistic, direct, dedup) | 30+ | ✅ Green |
| Security engines (rate limiter, quota, replay, reputation, spam, ACL) | 50+ incl. proptests | ✅ Green |
| Message engine (queue, dedup, fragment, ack, lifecycle, expiry) | 25+ | ✅ Green |
| Discovery (neighbor table, handshake, bloom exchange) | 10+ | ✅ Green |
| Gateway (health monitoring, selection matrix, failover) | 15+ | ✅ Green |
| Protocol codec (CBOR canonical, CDE integers, signing scope) | 15+ | ✅ Green |
| Observability (metrics registry, privacy short-ids) | 5+ | ✅ Green |
| Sim scenarios (dense mesh, partition carry, vehicle relay, pilot B1-B4) | 15+ | ✅ Green |
| Protocol conformance (RFC 8949 CDE, RFC 9171 timestamps, beacon resync) | 10 | ✅ Green |
| Golden vectors (V001/V002/V003 byte-exact corpus) | 6 | ✅ Green |
| Tokio behavior (paused-time async correctness) | 4 | ✅ Green |
| ML experiments (GT predictor, shadow features, LP agreement) | 8 | ✅ Green |
| Desktop shell (identity, peer-id hex, commands, engine roundtrip) | 14 | ✅ Green |
| iOS FFI bridge (BLE adapter projection, engine runtime, body surface) | 11 | ✅ Green |
| Storage (seal integrity, pg store, M3 engine roundtrip) | 21 | ✅ Green |
| BLE transport (discovery, GATT, segmentation, adversarial parse) | 42+ | ✅ Green |
| Wi-Fi Aware (NDP roundtrip, availability churn, outbox bounds) | 12+ | ✅ Green |
| Wi-Fi Direct (DNS-SD TXT, group management, outbox eviction) | 16+ | ✅ Green |
| Internet transport (TCP framing, connection pool, backoff jitter) | 8+ | ✅ Green |
| Property-based tests (ACL, quota, rate limiter, replay, reputation, spam) | 33 cases | ✅ Green (hermetic since frozen-clock fix) |

---

## 3. Issues found and fixed during testing

| ID | Severity | Issue | Fix |
|----|----------|-------|-----|
| LORA RT-101 | CRITICAL | Duty billed on payload only (18-B header unbilled) — Table-I exceedance reachable | Bill full encoded frame |
| SAT RT-101 | HIGH | P0 classification authority unbounded above transport | RECORDED as binding obligation |
| LORA RT-102 / SAT RT-102 | MEDIUM | Non-monotonic wall-clock vulnerable to steps | Monotonic CAS clamp (both guards) |
| LORA RT-103 | MEDIUM | Hot-plug open-failure ignored, connect promoted dead links | Result-returning attach + status probe |
| SAT RT-103 | MEDIUM | Blind newest-pop refunds mispaired | Exact-token removal + day-aware decrement |
| LORA RT-104 / SAT RT-104 | MEDIUM | Failed TX burned budget / backlog silent drop | Refund API + push_deferred over-cap re-insert |
| LORA RT-105 / SAT RT-105 | MEDIUM | Malformed frame aborted poll batch / 271–340 B blackhole | Skip-and-count + MT-relay caps advertised |
| SAT RT-106 | MEDIUM | Hook panic escaped task + forfeited admission | catch_unwind containment |
| SAT RT-108 | LOW | Hot-plug TOCTOU (attach/shutdown race, double-open leak) | Slot-CS shutdown recheck + close-on-replace |
| SAT RT-109 | LOW | Unbounded ledger growth on constrained devices | Bounded ring buffer 4096 events |
| LORA RT-107 | LOW | Optimistic oversized-window delay hint | Unified feasibility walk |
| rate_limiter flake | LOW | Proptests crossed real-time refill boundary under load | Injectable frozen clock for property tests |

---

## 4. LEGAL-001 disposition

LEGAL-001 was **deferred to the future backlog** by operator ruling
(iter ~178). Its anti-misuse purpose is preserved by the shipped technical
controls (SEC/EMERG/IDENT engines + transport hard gates). The legal-
execution items (WPC-ETA, GMPCS channel, STQC certification, MoU structure)
are catalogued in `docs/legal/CARRIED_OBLIGATIONS_REGISTER.md` with explicit
reactivation triggers (field pilot with TX-capable units, India market
release, government MoU signature). No legal opinion is rendered by any
project artifact.

---

## 5. Known limitations

| Limitation | Gate | Reactivation |
|---|---|---|
| Physical-device BLE/Wi-Fi tests (real radios) | BLK-0005 | Operator provides Android hardware |
| **Physical-device reliability** — the one bench session that showed BLE/Wi-Fi Direct delivery was *not* a reliability result; field use is intermittent | tracked in `docs/bug-hunting/hardware_verification/` (81 findings, opened 2026-08-31) | that pass re-verifies each transport with a Mobly multi-device harness + `btsnoop` evidence |
| Real-modem LoRa/SAT link benchmarks | BLK-0005 | Hardware procurement (EXP-LORA-001 Month 8) |
| India field activation of TX-capable units | DEC-SAT-0006 legal gate | Legal sign-off + authorized channel |
| Battery measurement on real hardware | EXP-003 | Physical-device phase |
| Throughput figures folklore-grade | G-5 | HARDWARE_VALIDATED benchmark required |
