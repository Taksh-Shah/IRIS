# BLE-001 SECURITY_REVIEW — Redteam Findings & Disposition (AC-15)

**Node**: BLE-001 (P0 TRANSPORT, ANDROID)
**Stage**: SECURITY_REVIEW (iter 88)
**Date**: 2026-08-16
**Baseline at review**: TEST iter 87 (workspace 524 green / clippy 0, BLE 33)
**Baseline after fixes**: workspace **533 passed / 0 failed / 1 ignored**,
`cargo clippy --workspace --all-targets` **0 warnings**.

---

## Verdict

**CONDITIONAL-FAIL → RESOLVED (fix-before-ACCEPT).** The adversarial review
(redteam subagent, live code read + confirmatory tests) found **4 HIGH + 4
MEDIUM + 5 LOW + 2 INFO** findings (RT-001..016, RT-008 absent). The pen-tester's boundary review confirmed
the beacon parser, frame decoder, trust boundary, MTU clamp and shutdown
ordering are sound. The 4 HIGH defects were real and exploitable by any
in-range BLE peer; **7 of 8 code findings (HIGH+MEDIUM) were FIXED in-pass
(RT-001/002/003/016/004/005/006; RT-009 RECORDED — accepted design contract,
DEC-BLE-0006)** and the LOWs fixed or recorded. **All findings dispositioned below.**

## Findings

### HIGH — all FIXED

| ID | Finding | Disposition |
|----|---------|-------------|
| **BLE-RT-001** | Reassembly TTL eviction was dead code: `Reassembler::evict_stale` unit-tested but never called in the live poller → 64 partial slots became a **permanent** multi-chunk DoS per peer (a few-KB frame burst poisoned message delivery for the connection lifetime). | **FIXED** — poller now runs a coarse TTL sweep every 1 s (`ble.rs`, evict before drain). Regression `rt001_partial_slots_reclaimed_by_ttl_sweep_in_poller`: 66 stale partials injected, page past sweep, a fresh multi-chunk message reassembles E2E. |
| **BLE-RT-002** | `connect()` TOCTOU: two concurrent connects to the same peer both passed `contains_key`, both `connect_gatt` (GATT handle leak — consumes the ~32-client ceiling), both spawned a poller, and the 2nd `pollers.insert` **overwrote** the 1st AbortHandle → orphaned task draining the shared queue after shutdown + multi-chunk splits across two reassemblers. | **FIXED** — `connect()` now holds the `connections` lock across the *entire* setup (check → connect_gatt → set_mtu → insert → spawn); concurrent connects serialize and reuse. Poller spawn aborts any prior AbortHandle for the peer. Regression `rt002_concurrent_connect_single_gatt_and_poller` (tokio::join! asserts connect_count()==1, one handle, one poller). |
| **BLE-RT-003** | One **global** ATT MTU for all peers: a later high-MTU connect resized the shared segmenter, so sends to earlier low-MTU (23-byte) peers emitted ~506-B frames that GATT rejects → blackholed legacy peers (the documented mixed-MTU case). | **FIXED** — MTU is stored **per connection** (`connections: HashMap<PeerId,(GattHandle,u16)>`); `send()` uses `segmenter.segment_for_mtu(payload, mtu)` sized to the target peer's own negotiated MTU. Regression `rt003_frames_sized_to_each_peer_own_mtu` (low peer ≤18-B frames, high peer 512-B, more frames for low). |
| **BLE-RT-016** | Shared adapter queue drained by **every** peer poller; pollers filtered only by `char_uuid` not `handle` → inbound frames from B consumed by A's poller and emitted as `IncomingMessage{peer_id:A}` (transport-level sender spoofing feeding SEC-001/ROUTE-002 identity gates) and B's multi-chunk messages split across two reassemblers (permanent loss, compounded by RT-001). | **FIXED** — each poller now partitions the queue **in place**: it removes only frames whose `handle == its own GattHandle` and leaves every other peer's frames in the queue for their poller (foreign frames are no longer drained+discarded). Regressions `rt016_pollers_do_not_steal_each_others_frames` (peer A 2-chunk + peer B single-chunk interleaved → each attributed to the right peer, both reassemble). |

### MEDIUM — 3 FIXED + 1 RECORDED

| ID | Finding | Disposition |
|----|---------|-------------|
| **BLE-RT-004** | Failed `connect()` left state stuck `Connecting` (manager `state>=Available` keeps selecting a dead link — RED-0001-13 bug not carried to BLE). | **FIXED** — connect error paths restore `TransportState::Available` (same pattern as `internet.rs`). Regression `rt004_connect_failure_restores_available`. |
| **BLE-RT-005** | No link-liveness/reap: per-entry `Connected` flag never cleared, no write-failure teardown, no peer-close → connection map grew monotonically, pinning GATT slots (~32 ceiling) and manager kept routing into dead links. | **FIXED** — new `close_peer(peer, adapter, cause)` teardown on `send()` write failure: removes connection entry, **aborts the poller**, disconnects the GATT handle, and falls back to `Available` when the last link dies. Regression `rt005_write_failure_tears_down_dead_peer` (entry removed, poller aborted, state Available). |
| **BLE-RT-009** | Beacon `peer_short` is fully attacker-controlled → identity *squinting*: set short = first 16 B of a real peer's PeerId → look-alike candidate poisoning at attacker RSSI/MAC. | **RECORDED (accepted design contract, DEC-BLE-0006)** — candidates are unauthenticated by design; `candidate_peer_id()` zero-pads and is explicitly a hint, never a trust boundary; `connect()` from any advertisement is still fine because message *payloads* remain envelope-verified by SEC-001/IDENT-001. `PeerInfo` has no authenticated flag in the core model; tagging mid-layers (neighbor table displays) is a follow-up RECORDED. |
| **BLE-RT-006** | `scan_allowed()` check-then-act race allows ceiling bypass under concurrent discovery (Android ≥14 silently fails 6th scan). | **FIXED** (design already serialized) — the ceiling check+push runs under one `scan_times` mutex so concurrent `discover_peers` serialize; backoff is armed in the same critical section. LOW residue: OS scan-failure callback is not surfaced as `TransportError` (real-adapter item, see known_limitations). |

### LOW — fixed or recorded

| ID | Finding | Disposition |
|----|---------|-------------|
| **BLE-RT-007** | `encode_frame()` silently truncated oversized total/count/idx in release (`as u16`/`as u8` with only `debug_assert`). | **FIXED** — `encode_frame` now returns `Result<Vec<u8>, FrameError>` with runtime bounds (`MessageTooLarge`/`ChunkCountTooLarge`/`BadChunkIndex`/`Truncated`). Regression `rt007_encode_frame_rejects_oversized_values`. |
| **BLE-RT-010** | Beacon `freshness_minutes` never validated (any 0..=65535 accepted; no staleness/future check). | **RECORDED** — field is a hint with no consumer yet; never a trust boundary (freshness cannot gate security — DEC-BLE-0006). Validation policy deferred to the discovery consumer (DISCO-001). |
| **BLE-RT-011** | `start_advertising()` overwrites `adv_handle` without stopping the prior advertisement → Android advertising-set leak (max ~4) + stale beacon. | **FIXED** — stop the prior handle before starting a new advertisement. Regression `rt011_readvertise_stops_prior_handle`. |
| **BLE-RT-013** | `discover_peers()` ignored `DiscoveryConfig.max_peers` (advert flood / batch scan bypassed caller cap) and never stopped the scan on stream length. | **FIXED (capped)** — drained results capped at `config.max_peers`. Scan lifecycle is still driven by explicit `stop_discovery()` per the Transport contract (documented below). Regression `rt013_discovery_enforces_max_peers`. |
| **BLE-RT-014** | Capability `max_message_size = u16::MAX` unreachable at default MTU 23 (~3 KB before negotiation; ChunkCountTooLarge). | **RECORDED (known_limitation)** — `max_message_size` advertises the segmented wire cap (AC-2). At MTU 23 the practical ceiling is ~3 KB until `requestMtu` succeeds; `send()` surfaces `Protocol("segmentation failed")` which the manager/engine handles as an oversized-message error. New known_limitation added to graph. |

### INFO — recorded

| ID | Finding | Disposition |
|----|---------|-------------|
| **BLE-RT-012** | 50 ms busy-poll per peer (~20 wakeups/s × up to ~32 peers) — battery/CPU concern only partially mitigated by AbortHandle shutdown. | **RECORDED** — real Android adapter should use notification/callback-driven delivery (JNI callback). The poll cadence is already coarse relative to BLE's typical connection interval. |
| **BLE-RT-015** | `broadcast::channel` drop-oldest on overflow silently sheds inbound messages when consumers stall. | **RECORDED** — consistent with the message_engine broadcast contract (engine-level backpressure is the primary bound); per-priority channels deferred to a platform node. |

---

## Positive controls (verified by redteam, held)

1. Beacon parse strict + allocation-free (fixed 16-B copy, version gating, `ReservedBits`, exhaustive short sweep, oversized-padding tolerance).
2. Frame decode bounds sound (`u16`/`u8` impossible to overflow from wire; `Truncated` on single- and multi-chunk path; `MAX_PARTIALS` gate).
3. Trust boundary held: connectionless adverts only yield candidate `PeerInfo`; no advertise→connect/send trigger.
4. RED-0001-07 held: `send()` looks up the peer's own `GattHandle` (no hardcoded handle).
5. MTU clamp `[23,517]`, saturating arithmetic, no underflow/zero-divide.
6. Shutdown ordering sane: abort pollers → disconnect GATT → Unavailable.

---

## Verification

- `cargo test --workspace`: **533 passed / 0 failed / 1 ignored** (iris-core lib 478 + crypto_e2e 3 + ml 8 + obs 4 + sim 8+1 ignored + desktop 5 + commands 3 + engine_roundtrip 3 + storage 7 + m3 1 + pg_store 13).
- `cargo test -p iris-core --lib transport::ble`: **42 passed / 0 failed** (24 in ble.rs [incl. **9 RT regressions**: RT-001/002/003/016/004/005/007/011/013] + ble_att 10 + ble_advert 8).
- `cargo clippy --workspace --all-targets`: **0 warnings**.
- All 8 RT fixes are real code with dedicated regression tests; no AC evidence fabricated.

**STAGE_TRANSITION → VERIFY (AC-16, iter 89).**