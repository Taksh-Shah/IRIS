# LORA-001 SECURITY_REVIEW — adversarial redteam review (AC-16)

**Document ID**: IRIS-LORA-001-SECURITY-REVIEW-001
**Date**: 2026-08-19 (iter ~165)
**Scope**: `crates/iris-core/src/transport/lora.rs` (full module incl. tests)
+ cross-check of the TEST-stage transient flake (`security/rate_limiter.rs`
proptests) + design contract `LORA_001_DESIGN.md` §3.
**Method**: independent redteam subagent, read-only adversarial pass against
the surfaces enumerated in NEXT_ACTION iter ~165; supervisor consolidated
dispositions and applied fixes in-pass. **Verdict: FAIL → RESOLVED**
(PASS-with-fix after RT-101..107/109 applied this iteration).
**Live verification after fixes**: workspace **695 passed / 0 failed /
1 ignored** (`--all-features`, full sweep), clippy `-D warnings` **0**, fmt
clean, `transport::lora` **31/31** (+4 new RT regressions).

---

## Findings and dispositions

### RT-101 | CRITICAL | Duty billed on payload only — Table-I under-count → FIXED

The transport charged `airtime_ms(profile, payload.len())` while the radio
transmits the encoded frame (18-B IRIS header + payload). Small frames were
under-billed ~44–79%; a minimal-frame flood across all four buckets was
arithmetically admitted at 35,776 ms accounted ≈ 63,984 ms true on-air (~1.78%
duty — 78% over the WPC G.S.R. 853(E) 2021 limit). Violated the design's own
"never under-count" rule (§2.1).

**Fix (this pass)**: `try_send_inner` encodes first and bills
`airtime_ms(&profile, encoded.len())`. SLIP escapes are host↔module carrier
framing, not on-air bytes, and are deliberately not billed (documented).
**Regressions**: `duty_bills_full_encoded_frame_not_just_payload_rt101`
(1-B payload books 186 ms = airtime(SF9, PL=19), not 104 ms; bulk flood bound
9 frames/hour enforced) + updated anchors: full-frame accounting is
SF9/255-B→1251 ms and SF12/255-B→9020 ms (`p0_send_rides_sf12_airtime_
accounting`). Design-doc §2.3 note records the corrected accounting basis.

### RT-102 | MEDIUM | Wall-clock window vulnerable to clock steps → FIXED (+ recorded residual)

Production clock was raw `SystemTime` millis: a backward step froze budget
aging; forward steps (NTP sync on RTC-less boots, which `unwrap_or(0)` makes
routine) instantly released the whole hourly window.

**Fix**: monotonic clamp in `DutyCycleTracker::now()` (accepted time never
decreases; backward steps ignored). **Recorded residual**: a genuine forward
wall-clock jump still ages out records early relative to real elapsed hours;
fully immune semantics require an Instant-based private timeline, deferred as
hardening (window semantics documented on the tracker).

### RT-103 | MEDIUM | Hot-plug lies: open() failure ignored; connect() promoted dead links → FIXED

`attach_adapter` ignored `open()` errors (HardwareGated/faulted dongles stored
behind Available state); `connect()` promoted to Connected with no link check;
`shutdown()` left the closed adapter in the slot.

**Fix**: `attach_adapter` returns `Result<(), LoRaLinkError>` and stores
nothing on open failure (state stays Unavailable); `connect()` probes
`adapter.status()` before promoting (probe failure → ConnectionFailed);
`shutdown()` takes the adapter out of the slot; trait docs now require
post-close `tx()/rx() == Closed`. **Regression**:
`dead_dongle_attach_is_rejected_not_available_rt103`.

### RT-104 | MEDIUM | Failed TX burned duty budget with no refund → FIXED

Reservation preceded `adapter.tx()`; hardware faults silently consumed the
legal hour (fail-safe legally, but a flapping CDC dongle becomes a budget
incinerator and telemetry couldn't distinguish burn from refusal).

**Fix**: `DutyCycleTracker::refund(class, airtime)` removes the newest
matching reservation when TX fails (refund only ever removes usage — no bypass
possible); new `tx_failures` metric distinguishes failure-burn from refusal.
Documented race caveat (identical concurrent reservations may refund
ambiguously by one slot — never under-counts below actual successful sends).
**Regression**: `tx_failure_refunds_duty_reservation_rt104`.

### RT-105 | MEDIUM | One malformed inbound frame aborted the poll batch → FIXED

`poll_inbound` returned Err on the first decode error, so any in-range node
transmitting SLIP-invalid garbage could deny ALL inbound delivery behind it.

**Fix**: malformed SLIP/frames are skipped + counted (`malformed_rx` metric +
tracing); hard errors reserved for adapter-level Io/Closed.
**Regression**: `malformed_inbound_frame_does_not_abort_poll_batch_rt105`
(garbage prefix + valid suffix ⇒ delivered 1, malformed_rx 1).

### RT-106 | MEDIUM | Unbounded backlog without size gate → FIXED

`enqueue_backlog` accepted arbitrary-size payloads and unlimited depth —
memory-exhaustion vector on constrained devices.

**Fix**: enqueue rejects payloads > `MAX_PAYLOAD_BYTES` (Protocol) and queue
depth is capped at `MAX_BACKLOG_ENTRIES = 256` (Busy when full);
`BacklogQueue::push` returns bool. Drain re-push always fits (entries came
from the same queue).

### RT-107 | LOW | Optimistic oversized-window delay hint + wrong drain doc → FIXED

Refusal delay for absolute-oversize requests returned the oldest expiry
instead of walking to true feasibility (spin-retry churn, no violation).
Unified the walk for both branches; corrected the `drain_backlog` doc comment
(walk continues past refusals per bucket-proportional fairness, DEC-LORA-0003).

### RT-108 | LOW | Receipt semantics unverifiable delivery claims → RECORDED

Raw-LoRa PHY is broadcast; `SendReceipt.peer_id` echoes the REQUESTED peer —
"handed to radio", never "delivered". Contract documented on `send()`;
downstream ACK/dedup layers remain authoritative (MSG-001). Inbound
zero-PeerId attribution re-audited safe (WAW-RT-007 precedent intact; hostile
frames cannot buy priority above the envelope layer because IncomingMessage
carries no trust-bearing metadata).

### RT-109 | LOW | RadioProfile fields unvalidated → FIXED (debug guard)

Degenerate profiles produced saturating-cast u64 airtime (fail-closed but
silent). `airtime_ms` now `debug_assert`s SF 7–12 / BW ∈ {125,250,500} /
CR-denom 5–8; release builds keep fail-closed saturation. Consts unchanged.

### RT-110 | LOW | decode_slip extracts only the FIRST frame → RECORDED

Real bridges coalesce serial batches; one-frame-per-`rx()` is now a written
implementation contract on `LoRaLinkAdapter` (with decode_slip scope note) so
BLK-0005 firmware authors buffer internally. Future helper
(`decode_slip_all`) noted, not needed for v1.

### RT-111 | INFO | Sim honesty audit → PASS

All simulated evidence correctly tagged SIMULATION_VALIDATED (module docs,
SimLinkBudget, status()); model ranges clamped to validity; AN1200.13 anchors
match pins. Residual nit recorded: sim LinkStatus telemetry is
indistinguishable from hardware once surfaced — future bridge firmware should
stamp origin=sim|hardware (BLK-0005 scope).

### RT-200 | LOW (cross-file) | rate_limiter proptest flake root cause → FIXED (option a)

Confirmed: `check_bucket`/`refill` called `Instant::now()` directly; under
post-compile CPU saturation a property run straddles the 1 s refill boundary
and `allowed <= burst` legitimately breaks. The auto-persisted seeds captured
timing luck, not logic bugs (`refill_tokens` is saturating + Kani-proven).

**Fix implemented**: injectable clock (`RateLimiter::with_clock`,
DutyCycleTracker `now_fn` pattern). All four proptests + non-sleeping unit
tests use a frozen clock (`frozen_limiter`) making them hermetic;
`rate_limiter_refill_after_interval` keeps the REAL clock intentionally (it
tests live refill across a 1 s sleep). Regression seed file stays checked in
(proptest best practice; replays green). Property bounds NOT weakened.

## Positive controls verified held

- No crypto code/imports added by LORA-001; envelopes transported opaque
  (DEC-LORA-0008); crypto_e2e m7 suite green post-fixes.
- No override knob exists on the duty tracker (constructor validation is the
  only path; `validate()` rejects >Table-I configs) — RT-101 fix strengthens
  rather than relaxes enforcement.
- Defensive parse: bounds-checked slices, 4096-B SLIP cap, no unwrap/panic on
  hostile input (redteam confirmed none reachable).
- P0/P1 exempt-from-drop and quota cap-exemption (SEC-001/EMERG-001) untouched.

## Stage verdict

FAIL (RT-101 CRITICAL) → **RESOLVED in-pass**: 1 CRITICAL + 4 MEDIUM + 2 LOW
FIXED, 2 LOW RECORDED, 1 INFO PASS, RT-200 deflake FIXED. All fixes carry
regression coverage (31 lora tests incl. 4 new RT regressions; rate_limiter
12/12 hermetic). AC-16 satisfied → VERIFY (AC-17) next.
