# WIFIDIRECT-001 SECURITY_REVIEW — Redteam Findings & Disposition (AC-15)

**Node**: WIFIDIRECT-001 (P1 TRANSPORT, ANDROID)
**Stage**: SECURITY_REVIEW (iter 107)
**Date**: 2026-08-17
**Baseline at review**: TEST iter 106 (workspace **601 passed / 0 failed /
1 ignored**; `transport::wifi_direct` **16** = 7 transport + 9 serv; clippy **0**)
**Baseline after fixes**: workspace **608 passed / 0 failed / 1 ignored**
(553 iris-core lib + 55 integration/sim; `transport::wifi_direct` **23** = 13
transport + 10 serv), `cargo clippy --workspace --all-features --tests`
**0 warnings**, `cargo fmt --check` **clean**.

---

## Verdict

**PASS — fixes applied in-pass; residual findings recorded as
known_limitations / ANDROID-001 FFI contract.**

Adversarial review of `crates/iris-core/src/transport/wifi_direct.rs`,
`wifi_direct_serv.rs`, transport wiring, and `WIFI_DIRECT_COST` (pattern
BLE-001_SECURITY_REVIEW.md / WIFIAWARE-001_SECURITY_REVIEW.md): **1 HIGH +
5 MEDIUM + 1 LOW** code findings **FIXED in-pass with 7 regression tests**
(RT-001 HIGH; RT-002/003/004/005/007 MEDIUM; RT-009 LOW doc; RT-012 INFO
folded into RT-003), and
**5 INFO/LOW** findings **RECORDED** (RT-006 / RT-010 / RT-011 / RT-013 /
poll cadence — known_limitations / ANDROID-001 FFI
contract / manager arbitration). The HIGH was **real** at the DNS-SD candidate
seam: an all-zero TXT-record short id mapped to the "unknown sender" sentinel,
and the sim's two-step register left an empty-TXT window — both closed. Every
fix is real code verified by `cargo test`; no AC evidence fabricated.

## Findings (WIFIDIRECT-001_RT-001..013)

### HIGH — FIXED

| ID | Finding | Disposition |
|----|---------|-------------|
| **WIFIDIRECT-001_RT-001** | **Zero-attribution reachable at the DNS-SD candidate seam.** Two paths deliver the "unknown sender" `PeerId([0u8;32])`: (a) `WifiDirectTxtRecord::parse` accepted an all-zero `peer_short`, so `candidate_peer_id()` of a **well-formed** record maps directly to the zero sentinel — any peer (or DNS-SD injection point) advertising `0x00..00` gets its frames attributed to "unknown"; (b) the sim's `register()` first inserted its tag with an **empty** TXT record and set the bytes in a second map op, a window in which a peer's advertisement exists with no attributable record → `sender = None` → zero fallback. The AC-4 test asserted attribution for the happy path only; the adversarial window was invisible to it. | **FIXED** — (a) `parse()` now rejects all-zero `peer_short` with new `TxtRecordError::ZeroShortId` (semantic gate after the wire-structure gates — too-short/long/version/reserved/kind still fire first), so the zero sentinel is unreachable via a well-formed record; (b) `register()` replaced with atomic tag+TXT: new `coordinator.alloc_tag()` + single-lock `upsert_peer(tag, name, txt)` insert the advertisement with its final TXT in one op — no empty-TXT window ever observable. Regressions: serv `zero_peer_short_rejected` (all-zero → `ZeroShortId`; any non-zero byte accepted); transport `advertised_txt_is_attributable` (post-advertise TXT present, parses, short id never all-zero). |

### MEDIUM — all FIXED

| ID | Finding | Disposition |
|----|---------|-------------|
| **WIFIDIRECT-001_RT-002** | Full outbox evicted the **globally**-oldest frame regardless of destination: one busy sender to peer X could evict frames already staged for a slow-draining peer Y (cross-destination starvation flight in the shared mesh outbox). | **FIXED** — `proxy_send` (full outbox) now evicts the oldest frame queued for the **same destination** as the incoming frame, falling back to global-oldest only when none matches (bounded length preserved, per-destination fairness). Regression `outbox_eviction_keeps_other_destinations` (128-cap split 64+64 across two destinations; +1 for dest-2 → dest-4 frames untouched, still bounded). |
| **WIFIDIRECT-001_RT-003** | Sim `join_group()` **manufactured a phantom group** when the target peer was not a group owner (`group_of_go = None → create_group` + implicit GO). A client could "join" a plain peer and the transport reported `Connected` + membership for a group that never existed on the peer — the pair would believe a GO relationship existed with no actual data scope. | **FIXED** — `join_group` now **requires** an existing GO (`Ok(group_of_go) or Err("peer is not a group owner")`); a phantom group is never created. Regression `join_non_go_peer_is_rejected` (client-only A; B connect → `ConnectionFailed`, `coordinator.groups` stays empty, no phantom group). |
| **WIFIDIRECT-001_RT-012** | (INFO, folded into RT-003) `join_group` silently **overwrote** an existing `group_id`: a client joining a *second* GO would tear down only its last group (in `remove_group`/`shutdown`), leaving the first group's membership dangling in the mesh. Conflicts with the single-group-per-adapter platform contract. | **FIXED** — explicit guard: joining a different group errors (`"already in a group"`); re-joining the current group is idempotent and reports existing membership. Requires a registered advertising tag before joining (missing `my_tag == 0` guard added). |
| **WIFIDIRECT-001_RT-004** | `connect()` reuse path (link already present) returned `Ok` **without re-promoting** `Connected`. The sim's availability watcher self-heals, but a **pull-only adapter** (no `availability_stream`) whose state drifted below Connected would stay stuck over a live link with no event to recover. | **FIXED** — the reuse branch now calls `set_state(Connected)` (defense-in-depth closure for pull-only adapters). Behavioral regression `connect_reuse_after_churn_recovers` (Connected → band-restrict Degraded → recovery → Connected, single link reused). |
| **WIFIDIRECT-001_RT-005** | `send()` on an unavailable adapter returned `NotSupported` but **left the group links registered** and the transport sitting "Connected" over a dead radio. Pull-only adapters receive no follow-up availability event, so the stale connection persists indefinitely; a subsequent `p2p_send` returns a non-link-loss error (`Busy`) that never tears down. | **FIXED** — `send()` on `!adapter.is_available()` now tears **all** links down before returning `NotSupported`; the last-link teardown lands on `Degraded` (never a false "Available", keeping pull-only adapters honest). Regression `send_on_unavailable_tears_down_links` (NotSupported + links empty + state Degraded). |
| **WIFIDIRECT-001_RT-007** | Zero-length payload **accepted outbound** (framed, acked into `SendReceipt`) but **dropped inbound** (`len == 0` guard in the poller) → the sender sees a delivery receipt for a frame the receiver silently discarded. | **FIXED** — `send()` rejects empty payloads with `TransportError::Protocol("empty payload")` (receive-path symmetry: accept-then-silently-lose is worse than a clean error). Regression `empty_payload_send_rejected`. |

### LOW / INFO

| ID | Finding | Disposition |
|----|---------|-------------|
| **WIFIDIRECT-001_RT-006** | `dropped_inbound` telemetry undercounts: a slow/lagging upstream subscriber is silently skipped by the shared `broadcast_stream` helper (`RecvError::Lagged \u2192 continue`) without a count. Telemetry-only undercount; transport correctness unaffected. | **RECORD** — shared transport helper (BLE/WIFIAWARE/WIFIDIRECT all use it); fixing it is cross-transport scope. Documented in known_limitations. |
| **WIFIDIRECT-001_RT-009** | `INCOMING_CHANNEL_CAPACITY` doc comment claimed "8 × 1 MiB" while the channel is 32 × 1 MiB (32 MiB worst case; WIFIAWARE's same-named constant documents 32 × 1 MiB). Doc drift, no behavioral impact. | **FIXED (doc)** — comment corrected to 32 × 1 MiB worst-case with the per-tick forwarding bound. |
| **WIFIDIRECT-001_RT-010** | `ensure_started()` is not latched: every `discover_peers`/`start_advertising` re-runs `adapter.start()` + `start_dns_sd()` (+ `spawn_poller`/`spawn_avail_watcher` are spawn-once guarded, but the adapter calls are not). Trait declares them idempotent; a real Android adapter must not re-register service advertisements per call (battery + duplicate-match events). | **RECORD** — ANDROID-001 FFI contract pins adapter `start`/`start_dns_sd` idempotence (same-session reuse), mirroring NEW-WA-RT-111. |
| **WIFIDIRECT-001_RT-011** | `RadioConflictGroup` (`transport/mod.rs`) is defined but the `TransportManager` does not yet arbitrate concurrent radio sharing (Wi-Fi Direct 2.4 GHz vs Wi-Fi Aware vs infra Wi-Fi). | **RECORD** — manager-level arbitration is TRANSPORT-001 scope; transports already gate on adapter availability (runtime lever), and `WIFI_DIRECT.md` documents the single-radio coex reality. |
| **WIFIDIRECT-001_RT-013** | The 1 MiB frame cap is re-declared per transport (`MAX_WIFI_DIRECT_MESSAGE_BYTES`, `MAX_NAN_MESSAGE_BYTES`, `MAX_FRAME_BYTES` in internet.rs) instead of one shared constant. | **RECORD** — shared-constant refactor follow-up (FUTURE_TRANSPORTS), not a bug; values are intentionally equal. |
| (n/a) | Poll cadence 10 ms while linked / 500 ms idle (`IDLE_POLL_MS`): a single active link drives a 100 Hz poll of the adapter. Bounded per-tick (8 frames), but battery-sensitive on device. | **RECORD** — accepted design tradeoff; `WIFI_DIRECT.md` §Battery doc covered; physical-device battery BENCH gated (AC-14/BLK-0005). |

## Positive controls (verified, held)

1. **TXT-record parse is strict + allocation-free** — fixed-width 22-B prefix, no length-derived allocation; exhaustive 0..22 short-length sweep, version/kind/reserved gates, 255-B platform cap (`TooLong`), and now the zero-short semantic gate (RT-001) — all bounded ops, no panic paths (RES-0021 Q8).
2. **Frame decode bounds sound** — `frame_payload_len` Option; truncated / oversized / `len==0` frames dropped pre-allocation; per-tick budget `MAX_FRAMES_PER_TICK` enforced in the transport poller over a local backlog (no silent drop, no unbounded per-tick work).
3. **Candidate-only trust boundary held** — discovery yields candidate `PeerId`s from the TXT-record short id; full identity is resolved on the verified advertisement/envelope path (DEC-WD-0007), never the P2P MAC; the "unknown" zero sentinel is no longer reachable from a well-formed record (RT-001).
4. **Write-path gating** — `connect()` gates on transport state + adapter availability (before and after group formation); `send()` gates on availability with link teardown (RT-005); `connect_gate` serializes the whole open-link critical section against `shutdown()` (shutdown latch + gate).
5. **Bounded queues** — incoming channel 32 × 1 MiB (32 MiB worst case); coordinator outbox 128 frames, per-destination eviction (RT-002); per-tick drain budget (`MAX_DRAIN_PER_CALL = MAX_FRAMES_PER_TICK`); `dropped_inbound` telemetry on undeliverable inbound frames.
6. **Lifecycle sane + resurrection-proof** — `shutdown()` aborts poller + availability watcher, removes the group, clears links, latches `shutdown_flag`; `ensure_started` re-checks the latch under the gate so a dead transport can never be brought back.
7. **Group model integrity** — no phantom groups (RT-003), single-group-per-adapter enforced (RT-012), membership bounded by `MAX_GO_CLIENTS` (software admission; vendor/HAL ceiling per G-WD-1).
8. **Log hygiene** — no secrets logged: passphrase is never carried in the sim/transport layer (WPA2 passphrase transport is via authenticated BLE per DEC-WD-0003); cost-model arithmetic is simple float mults (no overflow path beyond f64).

## Verification

- `cargo test -p iris-core --lib --all-features transport::wifi_direct`: **23 passed
  / 0 failed** — 13 transport (`registration_gating`, `discovery_finds_advertising_peer`,
  `band_restricted_go_creation_falls_back`, `group_roundtrip_delivers_payload`,
  `discovery_rearm_and_stop`, `shutdown_returns_to_unavailable`,
  `single_link_per_peer_and_bounded_table`,
  **+RT** `advertised_txt_is_attributable`, `outbox_eviction_keeps_other_destinations`,
  `join_non_go_peer_is_rejected`, `connect_reuse_after_churn_recovers`,
  `send_on_unavailable_tears_down_links`, `empty_payload_send_rejected`) + 10 serv
  (9 baseline + **+RT** `zero_peer_short_rejected`).
- `cargo test --workspace --all-features`: **608 passed / 0 failed / 1 ignored**
  (553 iris-core lib + 55 integration/sim across 15 suites; baseline 601 → +7 RT
  regression tests).
- `cargo clippy --workspace --all-features --tests`: **0 warnings**.
- `cargo fmt --check`: **clean**.
- All fixes are real code with dedicated regression tests; no AC evidence
  fabricated; every RT row maps to a `file:line` fix + test.

**STAGE_TRANSITION → VERIFY (AC-16, iter 108).**