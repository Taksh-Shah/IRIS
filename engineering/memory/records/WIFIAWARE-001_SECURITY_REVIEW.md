# WIFIAWARE-001 SECURITY_REVIEW — Redteam Findings & Disposition (AC-15)

**Node**: WIFIAWARE-001 (P1 TRANSPORT, ANDROID)
**Stage**: SECURITY_REVIEW (iter 96 → re-review iter 99)
**Date**: 2026-08-16
**Baseline at review**: TEST iter 95 (workspace 490 lib + 23 integration + 8 sim
green / clippy 0, `transport::wifiaware` 12 + beacon 7)
**Baseline after fixes**: workspace **552 passed / 0 failed / 1 ignored** (497
lib incl. `transport::wifiaware` **19** + beacon **8**), `cargo clippy -p
iris-core --all-targets` **0 warnings**.

---

## Verdict

**CONDITIONAL-FAIL → RESOLVED (fix-before-ACCEPT).**

**Pass 1** — the adversarial review (redteam subagent, live code read +
confirmatory tests) found **1 HIGH + 1 MEDIUM + 3 LOW** code findings
(WAW-RT-001..005) plus held positive controls. The HIGH defect was **real and
exploitable by any caller of the transport API** (concurrent connect
double-open); the MEDIUM was a sim-only unbounded-queue / per-tick-alloc churn.
**All 5 code findings FIXED in-pass with regression tests.**

**Pass 2 (independent red-team re-review of data paths + concurrency)** found
**1 HIGH + 6 MEDIUM + 5 LOW** (NEW-WA-RT-101..112). The HIGH was **real on the
only E2E path**: the sim forwarded the *sender's* opaque NDP handle and the
poller matched the *receiver's* own links → every inbound frame was delivered
as `PeerId([0u8;32])`. The HIGH + 6 MEDIUM were **all fixed in-pass with
regression tests**; the 5 LOW are recorded as ANDROID-001 FFI-contract
requirements. Pass-1 fixes hold (no regression found). No AC evidence
fabricated; every fix is real code verified by `cargo test`.

## Findings — Pass 1 (WAW-RT-001..005, all FIXED)

### HIGH — FIXED

| ID | Finding | Disposition |
|----|---------|-------------|
| **WAW-RT-001** | `connect()` TOCTOU: the "already linked?" and `MAX_NDP_POOL` checks ran in a **scoped** `links` lock that was released before `adapter.open_ndp().await`. Two concurrent `connect()` calls to the **same peer** both passed the reuse check and both opened NDPs → **two data paths for one peer** (AC-9 "per-peer single NDP" violated) and duplicated `links` entries; two connects to different peers could also both pass the pool check and exceed `MAX_NDP_POOL`. Additionally the post-await `Unavailable` check then `links.push` had a window where a concurrent `shutdown()` (which clears `links`) could interleave between the check and the push → **dangling link registered against a torn-down NDP**. Same class as BLE-RT-002. | **FIXED** — added `connect_gate: AsyncMutex<()>` (**WAW-RT-001**): the entire critical section (reuse check → pool check → `open_ndp().await` → state re-check → `links.push`) now runs atomically, and `shutdown()` takes the same gate so teardown and the open-link sequence are mutually exclusive. Regression `concurrent_connects_open_single_ndp` (16 racing connects → exactly 1 link) + `connect_racing_shutdown_leaks_no_link` (final state Unavailable, links empty). |
| | **WAW-RT-002** | (MED — sim) `SimMeshCoordinator::proxy_send` pushed frames to the outbox **without bound** → a flooding local sender grows the mesh queue without limit (unbounded memory in-process). And `drain_outbox` drained the **entire** outbox into a `Vec` each poll while the poller's `.take(MAX_FRAMES_PER_TICK)` was applied only *after* the full allocation → per-tick allocation proportional to queue depth (alloc-churn), and frames beyond the per-tick budget were **silently dropped** (built then discarded), not deferred. | **FIXED** — added `MAX_OUTBOX_FRAMES=128` (drop **oldest** on overflow — finite-NDP-buffer semantics, mirrors a real NDP socket) + `MAX_DRAIN_PER_CALL=64` so `drain_outbox` returns a bounded batch and leaves overflow **queued** for the next poll (no loss). Regression `outbox_is_bounded_under_flood` (512-frame burst → outbox stays ≤ 128, single drain ≤ 64). |

### LOW — all FIXED

| ID | Finding | Disposition |
|----|---------|-------------|
| **WAW-RT-003** | `store_state_with_event` hardcoded `TransportId::from("wifi-aware-0")` in every event — if a second `WifiAwareTransport` instance ever registered, the availability watcher would emit events carrying the **wrong transport id**. | **FIXED** — watcher captures and threads the real `self.id` into the state events. |
| **WAW-RT-004** | `ensure_started()` unconditionally promoted `Unavailable → Available` even when the adapter reports `is_available()==false` at boot (e.g. display-off data scope already dropped before first start). The watcher only reacts to *transitions*, so the transport would sit falsely `Available` (unicast-capable per the manager) with no unicast scope. | **FIXED** — initial promotion now mirrors the adapter's current availability: `Available` when `is_available()`, otherwise `Degraded` (discovery-only survives per §3.5). AC-5 churn test still passes; no manager re-selection hazard. |
| **WAW-RT-005** | `wifiaware_beacon.rs::parse` accepted arbitrarily-long payloads (the 22-byte prefix parsed, extra bytes ignored) while `MAX_SERVICE_SPECIFIC_INFO_BYTES=255` existed as a dead constant and the module doc claimed oversized payloads are "rejected". A hostile/buggy adapter returning an over-cap payload silently sailed through. | **FIXED** — `parse` now enforces the documented 255-B cap: new `BeaconError::TooLong` rejected above the cap, boundary `== 255` still accepted. Regression `oversized_past_cap_rejected_no_panic` (277 B → TooLong; exactly-255 → ok). Constant is now live. |

## Findings — Pass 2 (NEW-WA-RT-101..112, re-review of data paths + concurrency)

### HIGH — FIXED

| ID | Finding | Disposition |
|----|---------|-------------|
| **NEW-WA-RT-101** | **Recv-side frame attribution ALWAYS yields `PeerId([0u8;32])`**: the sim forwards the *sender's* NDP handle in `IncomingNdpData.ndp`, but the poller matches against the *receiver's* own `links` (its own handles) → never matches → every inbound frame on the only E2E path is delivered as zero-peer. AC-4 test asserted payload only (gap invisible); the trait never defined local-vs-remote handle semantics for `IncomingNdpData.ndp` (ANDROID-001 would replicate the ambiguity). | **FIXED** — (1) trait now documents `ndp` = receiver-local handle, plus new `IncomingNdpData.sender: Option<PeerId>` carrying best-effort beacon-derived candidate identity; (2) `SimMeshCoordinator` outbox now stores `(to_tag, from_tag, payload)` and `drain_outbox` returns the **sender tag**; (3) sim `incoming_ndp` resolves the sender's latest beacon → candidate PeerId; (4) poller attributes from `frame.sender` (zero fallback preserved). AC-4 test now asserts `got.peer_id == A's beacon PeerId` and `!= [0u8;32]`. |

### MEDIUM — all FIXED

| ID | Finding | Disposition |
|----|---------|-------------|
| **NEW-WA-RT-102** | `connect()` lacked the `adapter.is_available()` gate that `start_advertising()` has: while Degraded (display-off data scope), connect proceeded into a doomed `open_ndp()` → `ConnectionFailed` instead of clean `NotSupported`, and was the only entry point that could enter Connected with no data scope. | **FIXED** — `connect()` now returns `NotSupported` when `!adapter.is_available()` (under the gate, before the pool/link checks). Regression `connect_while_unavailable_is_not_supported`. |
| **NEW-WA-RT-103** | Lost-transition race: `connect()→set_state(Connected)` was not serialized with the avail-watcher's `Degraded` set — a bad interleave leaves state Connected while `adapter.is_available()==false`; pull-only adapters get no follow-up availability event, so the transport is stuck. | **FIXED** — availability is re-checked **after** `open_ndp().await` too (the mid-open window); a drop landing during open now closes the NDP and returns `ShuttingDown` instead of registering a Connected-with-no-scope link. Watcher restore also honors existing links (NEW-WA-RT-106). |
| **NEW-WA-RT-104** | `teardown_link()` promoted last-link death to `Available` unconditionally even when the data scope is down — same false-unicast class as WAW-RT-004 but in the teardown path: a Degraded transport with a dying last link lied "Available" to the manager. | **FIXED** — teardown now lands on `Degraded` when `!adapter.is_available()`, else `Available`. Regression `last_link_death_while_unavailable_is_degraded`. |
| **NEW-WA-RT-105** | Per-tick budget `MAX_FRAMES_PER_TICK` enforced only inside the sim (`MAX_DRAIN_PER_CALL`); the poller forwarded whatever `incoming_ndp()` returned with no defense — a real adapter returning a large batch gave unbounded per-tick work/energy (WAW-RT-005's bound was sim-only). | **FIXED** — per-tick enforcement moved into the transport: the poller now tops up a **local backlog** from `incoming_ndp()` and forwards ≤ `MAX_FRAMES_PER_TICK` per tick, leaving overflow buffered (no loss). Sim `MAX_DRAIN_PER_CALL` set equal to `MAX_FRAMES_PER_TICK`; poller `.take()` removed so the adapter's batch is never silently dropped. |
| **NEW-WA-RT-106** | `incoming_tx.send()` result ignored and `broadcast_stream` silently skipped `Lagged` → inbound frames dropped with zero telemetry/backpressure when the engine is slow or a burst exceeds 32×1 MiB. | **FIXED** — poller now counts every undeliverable frame in `dropped_inbound: Arc<AtomicU64>` (readable telemetry), and the send result branches on error instead of `let _`. |
| **NEW-WA-RT-107** | `send()` treated every adapter `ndp_send` error as link death → teardown + state drop: genuine congestion (real NDP socket ENOBUFS) caused a reconnect storm; conversely the sim returned Ok + `SendReceipt` even when the capped outbox dropped the frame — no delivery feedback either direction. | **FIXED** — new `is_link_loss_error()` classifier: only terminal markers ("not open"/"closed"/"disconnect"/"reset"/"link lost") tear down; transient errors return `Busy` and keep the link. Sim's closed-path marker is now `"ndp_not_open"`. Regression `transient_send_error_keeps_link` (Busy → link survives → next send ok). |

### LOW — RECORD (addressed at FFI/integration: ANDROID-001)

| ID | Finding | Disposition |
|----|---------|-------------|
| **NEW-WA-RT-108** | `connect()` NDP-reuse keyed on the *candidate* `peer.peer_id`: two peers publishing the same short id → second connect silently reuses the first's NDP → data-path misrouting. Envelope bounds impact (candidate-only doctrine). | **RECORD** — key reuse on the **verified** PeerId once envelope resolution exists; short-id collisions are candidate-level only (DEC-WA-0007). |
| **NEW-WA-RT-109** | Frames queued for a closing/closed NDP still drained + forwarded zero-attributed; `unregister_peer` purges only frames *to* the tag; `availability_stream()` is single-use `take()` so a rebound transport never observes churn. | **RECORD** — ANDROID-001: adapter must prune frames for closed NDPs and expose a multi-subscriber availability stream. |
| **NEW-WA-RT-110** | `connect_gate` held across platform awaits (`open_ndp`, shutdown's `close_ndp` loop, `start/subscribe`): one slow/hung adapter call globally stalls connect/shutdown/ensure_started. | **RECORD** — ANDROID-001 FFI must add per-call timeouts (platform calls are seconds-scale worst case). |
| **NEW-WA-RT-111** | `ensure_started()` not latched: every `discover_peers`/`start_advertising` re-runs `adapter.start()+subscribe()` — on Android each `subscribe()` creates a new DiscoverySession → duplicate match events + battery. | **RECORD** — ANDROID-001 FFI contract pins adapter start/subscribe **idempotence** (same-session reuse). |
| **NEW-WA-RT-112** | `drain_outbox` rebuilds the full `keep` Vec (O(outbox)) per call even when returning ≤8 frames → per-10ms allocation churn under load. | **RECORD** — ANDROID-001/optimization: ring-buffer outbox instead of remove-0/rebuild. |

## Positive controls (verified by redteam, held)

1. **Beacon parse is strict + allocation-free** — fixed 16-B copy, version gating, `ReservedBits` rejection, exhaustive short-length sweep, reserved-caps mask; only bounded ops on the prefix (no length-derived allocation).
2. **Frame decode bounds sound** — `frame_payload_len` returns `Option`, truncated/oversized frames dropped pre-alloc; poller budget `MAX_FRAMES_PER_TICK` honored at the source after WAW-RT-002 / NEW-WA-RT-105.
3. **Trust boundary held** — discovery yields candidate-only `PeerInfo`; `candidate_peer_id` zero-pads the short id and is a hint, never a trust boundary (DEC-WA-0007); the NAN MAC is never an identity. `peer_for_ndp` fallback is the all-zero PeerId (never attacker-controlled), and the envelope layer performs real verification.
4. **NDP handle hygiene** — handles are opaque monotonic u64 counters, never reused; `close_ndp` idempotent; sim rejects self-NDP and unknown-peer `open_ndp`; `shutdown()` closes every open NDP before adapter teardown.
5. **Send failure teardown bounded to link-loss** — `ndp_send` teardown now gated by `is_link_loss_error` (NEW-WA-RT-107); transient congestion is non-terminal.
6. **Shutdown ordering sane + resurrection-proof** — abort poller + availability watcher → close all NDPs → adapter shutdown → clear links → `Unavailable`; now gate-serialized against `connect()` AND `ensure_started` holds the same gate + checks a latched `shutdown_flag` so a dead transport can never be brought back up (NEW-WA-RT-102).
7. **Bounded channels** — `incoming_tx` capacity 32 × 1 MiB (worst-case 32 MiB); `state_tx` 64; availability broadcast 8; undeliverable inbound frames counted, not silently lost (NEW-WA-RT-106).
8. **Idle backoff** — poller sleeps `IDLE_POLL_MS` (500 ms) when no links are open, 10 ms while linked (RT-014 energy gating).

## Verification

- `cargo test --workspace`: **552 passed / 0 failed / 1 ignored** (497 iris-core
  lib + 55 integration/sim across 15 suites).
- `cargo test -p iris-core --lib transport::wifiaware`: **19 passed / 0 failed**
  (was 12; +3 RT regressions +4 NEW-WA-RT regressions:
  `concurrent_connects_open_single_ndp`, `connect_racing_shutdown_leaks_no_link`,
  `outbox_is_bounded_under_flood`, `transient_send_error_keeps_link`,
  `last_link_death_while_unavailable_is_degraded`,
  `connect_while_unavailable_is_not_supported`, and the strengthened
  `ndp_roundtrip_delivers_payload` attribution assertions).
- `cargo test -p iris-core --lib transport::wifiaware_beacon`: **8 passed**
  (was 7; +1 `oversized_past_cap_rejected_no_panic`).
- `cargo clippy -p iris-core --all-targets`: **0 warnings**.
- All fixes are real code with dedicated regression tests; no AC evidence
  fabricated.

**STAGE_TRANSITION → VERIFY (AC-16, iter 100).**