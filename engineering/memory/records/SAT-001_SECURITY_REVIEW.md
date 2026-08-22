# SAT-001 SECURITY_REVIEW — adversarial redteam review (AC-16)

**Document ID**: IRIS-SAT-001-SECURITY-REVIEW-001
**Date**: 2026-08-22 (iter ~173)
**Scope**: `crates/iris-core/src/transport/satellite.rs` (full module incl.
tests) with **MANDATORY receive-path focus** per RES-0028 G-2 (adversary model
changed: SIM-cloning + spoofable-downlink proven by arXiv:2603.12062).
**Method**: independent redteam subagent, read-only adversarial pass;
supervisor consolidated dispositions and applied fixes in-pass.
**Verdict: FAIL → RESOLVED** (PASS after RT-102..106 + LOW 108/109 applied
this iteration). **Live verification after fixes**: workspace **720 passed /
0 failed / 1 ignored**, clippy `-D warnings` **0**, fmt clean,
`transport::satellite` **24/24** (+3 new RT regressions),
`transport::lora` backlog deferred test green.

---

## Findings and dispositions

### SAT-RT-101 | HIGH | P0 SOS-exemption driven by caller-supplied metadata → RECORDED (binding above-transport obligation)

`SerializedMessage.priority` is caller-supplied with no classification
authority in-core; P0 bypasses every spend control unconditionally (by design,
DEC-SAT-0003). A compromised component stamping spam as P0 drains prepaid
money on the last-resort transport. External attackers cannot reach send()
directly (local API).

**Disposition**: RECORDED as a binding trust-boundary obligation — P0
classification authority MUST be enforced ABOVE the transport (only
EMERG-001-classified origins may set priority=P0; envelope priority field is
the enforcement point). Alarm-only observability added this pass: emergency
admissions are ledger-visible (`TxAdmitted { emergency: true }`) and token 0
marks them structurally unrefundable. Never blockable by design.

### SAT-RT-102 | MEDIUM | CostGuard lacked the RT-102 monotonic clamp (lesson-class recurrence) → FIXED

A backward wall-clock step across a day boundary reset `p12_today`/
`mailbox_today` (exhausted ₹200/day fully restored = double-spend) and froze
hourly pruning. Exactly the lesson class LORA RT-102 closed — it recurred.

**Fix**: monotonic `last_ms` CAS floor ported into `SatelliteCostGuard::now()`;
accepted time never decreases. **Regression**:
`guard_backward_clock_step_cannot_reset_budgets_rt102`.

### SAT-RT-103 | MEDIUM | Blind newest-pop refunds mispair under late/expired/day-crossed scenarios → FIXED

`refund_tx` popped the newest slot unconditionally: late refunds released an
UNRELATED fresh slot (net extra send past cap); day rollover mid-flight
decremented TODAY's counter for yesterday's send; spurious refunds corrupted
the AC-3 audit trail (ledger recorded even when nothing was refunded).

**Fix**: admissions now carry unique tokens (`GuardAdmission.token`);
`refund_tx(token)` performs exact-token removal, decrements today's counter
only when the admission happened today, records `TxRefunded` only when a live
reservation was actually refunded, and returns bool. Emergency admissions
(token 0) are structurally unrefundable. **Regression**:
`guard_refund_exact_token_no_mispair_rt103` (uniqueness, exact release,
duplicate-refund no-op, unknown-token no-op).

### SAT-RT-104 | MEDIUM | drain_backlog deferred re-push could silently DROP held P0–P2 traffic → FIXED

When concurrent enqueues refilled the queue during a drain, deferred entries
hit the depth cap and were silently dropped (`let _ = push(...)`), including
emergency holds — an AC-4 queue-not-drop violation. The same defect existed in
lora.rs with a false "re-queue always fits" comment.

**Fix**: `BacklogQueue::push_deferred` ignores the cap for items already
queued once (temporary over-cap until drains recover); both transports'
drain paths use it; false comment deleted. **Regression**:
`backlog_push_deferred_over_cap_retains_held_traffic_sat_rt104` (lora).

### SAT-RT-105 | MEDIUM | 271–340 B envelope one-way blackhole → FIXED

Caps advertised MO 340 while every SBD MT receiver hard-drops >270:
mesh-relayed 271–340 B envelopes were structurally undeliverable while the
sender believed custody was handed off.

**Fix**: capabilities advertise the MT budget (`max_message_size = 270`,
SAT-RT-105 test updated) so manager selection and MSG-001 fragmentation
thresholds account for the relay leg; MO 340 documented as gateway-egress-only
(dead zone explicit in code comments + design §2.2 correction staged).
Transport + enqueue gates aligned.

### SAT-RT-106 | MEDIUM | Confirmation-hook panic escaped the task AND forfeited the reservation → FIXED

A panicking user hook unwound out of send() before refund: per-panic budget
leak plus tokio-task death. Sync hook also blocks the worker.

**Fix**: hook invocation wrapped in `catch_unwind(AssertUnwindSafe(...))`;
panic treated as DENIAL → refund + `confirmation_denied` counter + Busy.
Auto-confirm default (no UI installed) documented honestly in design
(DEC-SAT-0003 note). **Regression**:
`confirmation_hook_panic_treated_as_denial_rt106`.

### SAT-RT-107 | MEDIUM | Mailbox-check billing attacker-influenceable; RX spend uncapped → RECORDED

With downlink spoofing proven, Ring-Alert-triggered polls can be induced at
will; checks bill unconditionally and total spend has no hard backstop. The
budget-aware cadence obligation (design §5) is above-transport.

**Disposition**: RECORDED as binding follow-up tied to RES-0028 FC-7:
minimum poll-interval/backoff knob + daily-spend alarm threshold
(`daily_spend_inr()` surfaced this pass) belong to the poll-cadence owner;
in-core never-refused semantics retained deliberately (skipping checks loses
inbound).

### SAT-RT-108 | LOW | Hot-plug TOCTOU set → FIXED

attach/shutdown race could re-promote Available post-shutdown; double-open
dropped the previous adapter without close (serial-port leak); connect probe
raced detach leaving transient Connected-without-link.

**Fix**: shutdown re-checked INSIDE the slot critical section before
store/promote; replaced adapters closed on swap; connect re-verifies adapter
presence after the probe. Mirrors lora RT-103 hardening.

### SAT-RT-109 | LOW | Default InMemoryLedger grew without bound → FIXED

Every mutation (including per-poll mailbox checks) appended forever on a
constrained device.

**Fix**: bounded ring (`IN_MEMORY_LEDGER_CAP = 4096`, drop-oldest) + manual
Default; durable sinks remain the production persistence path.

### SAT-RT-110 | INFO | Receive-path positive controls verified → PASS

(a) Hostile pipe HOLDS: poll_inbound forwards envelopes verbatim w/
zero-PeerId attribution; NO content inspection/trust below CRYPTO-001.
(b) Defensive parse O(1): length-checks precede allocation; malformed frames
skip-and-count without aborting the batch (RT-105 applied). (c) Eligibility
gates duplicated send-side AND enqueue-side; queued priorities immutable.
(d) RT-103 lessons present (open-failure rejection, status probe, slot-take);
RT-104 refund verified. (e) Honesty: handed-to-radio receipts, Ring Alerts
hint-only, best-effort framing all documented.

## Positive controls summary

- No crypto code/imports; envelopes opaque; crypto_e2e green in sweep.
- Eligibility gate duplicated (send + enqueue); queued priorities immutable.
- Refunds only ever REMOVE spend; P0 unbudgeted by ratified design
  (DEC-SAT-0003) with new observability.
- All LORA security-review lesson classes checked for recurrence; one found
  (RT-102 class) and fixed.

## Stage verdict

FAIL → **RESOLVED in-pass**: 5 MEDIUM + 2 LOW FIXED (RT-102/103/104/105/106 +
108/109), 1 HIGH + 1 MEDIUM RECORDED as binding above-transport obligations
(RT-101/107), 1 INFO positive-controls PASS. All fixes carry regression
coverage (transport::satellite 24/24 incl. 3 new RT regressions; lora backlog
deferred test). AC-16 satisfied → VERIFY (AC-17) next.
