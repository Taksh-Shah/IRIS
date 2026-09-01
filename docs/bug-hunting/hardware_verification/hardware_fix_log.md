# Hardware Verification — Execution Log

**Owner:** Taksh Shah
**Companion:** [`hardware_problems.md`](hardware_problems.md) (state) ·
[`hardware_problems_loop.md`](hardware_problems_loop.md) (protocol)

One entry per **bench session** (a block of work, usually with two physical phones
on hand). Within a session, one sub-heading per finding worked. Every finding
sub-heading must, before it can be marked `🟢` in the tracker, contain:

1. **Research note** — the Phase-R output (mechanism · why the code is wrong ·
   how others solve it · IRIS constraints · research outcome), with sources.
2. **Attempts** — `❌ Attempt N` for each hardware-test failure, with the
   `adb logcat` evidence and what it revealed.
3. **Hardware evidence** — the passing run: device models + Android versions,
   the scenario, N/N reliability, time-aligned `logcat` excerpts from both
   phones.
4. **Landed** — commit hash(es), files changed, docs corrected.

Sim-only progress is recorded as `✅ HW PENDING` and does **not** get a `🟢`.

---

## Log format template (copy for each session)

```
## Session <NN> — <YYYY-MM-DD>

**Devices:** P1 = <model> (Android <ver>, <chipset if known>) · P2 = <model>
(Android <ver>) · [P3 = <model> — multi-hop only]
**Build under test:** <git short-hash> (<branch>)
**Baseline captured:** cargo build --workspace = <result>;
./gradlew :app:assembleDebug = <result>; cargo test -p iris-android = <result>
**Session goal:** Tier <t>, findings HV-<a>..HV-<b>

---

### HV-<n> — <title>

**Phase R — Research note**
- Mechanism: …
- Why the current code is wrong: … (quote `file:line` at HEAD)
- How this is solved elsewhere: … (sources: …)
- IRIS constraints: …
- Research outcome: …

**Phase D — Implementation**
- Files changed: …
- Sim fault-injection test added: …
- `cargo build` / `cargo test` / `gradlew assembleDebug`: …

**Phase T — Hardware test**
- Procedure run: …
- ❌ Attempt 1: <what happened>
  ```
  <logcat P1>
  <logcat P2>
  ```
  Revealed: …
- ❌ Attempt 2: …
- ✅ Passing run: <scenario>, <N>/<N> consecutive, conditions <1m / 5m / 1 wall>
  ```
  <time-aligned logcat excerpt, both phones>
  ```

**Phase C — Close**
- Commit: <hash> — <subject>
- Tracker: HV-<n> → 🟢
- Docs corrected: <doc> — <what changed>

---

### Session <NN> closeout
- Findings advanced: …
- 🟢 count: <before> → <after>
- Blockers opened: HV-<x> (🔒 <reason>)
- New HV-## candidates spotted (not fixed): …
- Next session should pick up: …
```

---

## Session 00 — 2026-08-31 — Tracker bootstrap (no hardware)

**Devices:** none (documentation session)
**Build under test:** `6efd3a2` (main)
**Session goal:** create the three hardware-verification tracking documents; do
the codebase deep-dive that seeds `hardware_problems.md`.

### What was done

Read the transport / FFI / routing / UI layers to ground the findings in current
source:

- `crates/iris-core/src/transport/{ble,wifi_direct,manager}.rs`,
  `crates/iris-android/src/engine.rs`,
  `android/app/src/main/kotlin/iriscore/adapter/{AndroidBleTransportAdapter,AndroidWifiDirectTransportAdapter,SocketDataPath}.kt`
- `android/app/src/main/kotlin/iriscore/ui/{screens/ConsoleScreen,MeshViewModel,components/IrisConsoleInput}.kt`,
  `.../data/MeshRepository.kt`, `.../command/CommandExecutor.kt`,
  `.../util/PeerIdCodec.kt`, `.../service/ScanRestartPolicy.kt`
- `crates/iris-core/src/routing/flood.rs`, `message_engine/mod.rs`
  (`process_incoming` / `deliver_or_relay` / `enqueue_relay` /
  `spawn_delivery_loop`)
- Existing trackers: `docs/bug-hunting/README.md`, `taksh_problems_loop.md`,
  `docs/testing/{SYSTEM_TEST_REPORT,ANDROID_BUILD_STATUS}.md`

### Findings recorded

65 findings, HV-1..HV-64, across 9 tiers. See `hardware_problems.md`. Highlights
of what the deep-dive surfaced beyond the operator's report:

- **HV-1**: `crates/iris-android/src/engine.rs` test module calls
  `IrisEngine::new` with 4 args; the constructor takes 5 (`signer` added later).
  The Android engine's own round-trip test does not compile — the one test that
  supposedly proves an end-to-end FFI send has not run.
- **HV-42**: `grep -i 'internet|gateway|tcp' crates/iris-android` → **nothing**.
  The Android engine registers exactly BLE + Wi-Fi Aware + Wi-Fi Direct.
  `internet.rs` exists in core but has no Android FFI adapter and is never
  registered. The operator's "internet doesn't work" is literally "internet was
  never wired on Android." This is a build-out (Tier 5), not a bug fix.
- **HV-7 / HV-8**: `BleTransport` advertises `max_message_size` off the 23-byte
  default MTU; real messages fragment into many serial blocking ATT writes;
  `send()` calls `close_peer()` on the first fragment failure (no fragment-level
  retry). Accept-poller (peripheral-role) connections are pinned at `MTU_DEFAULT`
  (23) for the session — the reply direction runs at the worst MTU. This is the
  strongest single explanation for "sometimes sends, sometimes not" and "reply
  fails right after a successful receive."
- **HV-9**: the BLE beacon rides Manufacturer-Specific Data under SIG test id
  `0xFFFF`; foreign `0xFFFF` traffic is fed to `DiscoveryBeacon::parse` and can
  evict real beacons from the 512-deep oldest-drop scan queue.
- **HV-14**: `scan_allowed()` refuses the 6th scan start in 30 s and backs off up
  to 30 s. Reconnect churn (each `close_peer` → re-discovery → another start)
  drives the node into a 30-second discovery-blind window — a plausible "works,
  then dead ~30 s, then works" mechanism.
- **HV-19**: `AndroidWifiDirectTransportAdapter.createGroupWithBand` always builds
  a **named autonomous GO with per-instance random credentials**; the core's
  `go_intent` is ignored on this path. Two symmetric peers both self-elect as GO
  and can never join each other — the operator's "Wi-Fi Direct creates conflict."
- **HV-31**: the BLE adapter has **no** `BluetoothAdapter.ACTION_STATE_CHANGED`
  receiver (the Wi-Fi Direct adapter does have its P2P-state receiver). Toggling
  Bluetooth off/on leaves IRIS's scanner/advertiser/GATT-server handles stale
  with nothing rebuilding them; `ensureGattServer()` early-returns on the stale
  non-null server.
- **HV-35..37**: multi-hop relay, flood fan-out, and PRoPHET are **simulator-only**
  (`sysval_dtn_multihop`, `sim_scenarios`). Never run on physical radios. Blocked
  on a 3rd phone. The core relay path (`enqueue_relay` + `spawn_delivery_loop`)
  does exist and is wired — it has just never carried a byte over a real radio at
  hop 2.
- **HV-38**: `RelayOutbox` (Android) is the *local sender's* failed-send retry
  queue drained every 15 min by WorkManager — **not** the core mesh relay queue.
  Naming has been conflating two different things; the "Q3" chip reflects only the
  local outbox.
- **HV-54 / HV-55**: `ConsoleScreen`'s `LazyColumn` reserves a **static** bottom
  `contentPadding` (`InputHeight + XXL`) while `IrisConsoleInput` grows upward to
  `maxLines = 5` — the composer covers the newest messages. There is **no send
  button** at all; the only send path is the soft keyboard's `ImeAction.Send`,
  which many keyboards/layouts don't render as expected.
- **HV-56 / HV-57**: `CommandExecutor.setRecipient` requires exactly 64 hex
  characters. No `ContactStore`, no nickname map, no discovered-peer picker, no
  reply-to-set-recipient on a received message. `MeshUiState` exposes no peer
  list. Every message needs a pasted 64-hex id.
- **HV-58**: `engine.sendText` returning `Ok` means "queued," not "sent" or
  "delivered." The UI shows the message as sent immediately (HW-2) and if no route
  is ever found it silently expires after `ttl_seconds` (3600) with the UI still
  showing "sent." The core has `message_engine/ack.rs`; it is not surfaced to
  Android.
- **HV-46**: `select_transports`' `RadioConflictGroup` puts BLE in
  `Bluetooth24GHz` and Wi-Fi Aware/Direct in `WiFi24GHz` — it treats Bluetooth
  and 2.4 GHz Wi-Fi as non-conflicting and will run both hot for P0 multipath.
  Physical 2.4 GHz coexistence interference has never been measured.
- **HV-5**: `FfiIncomingMessage.received_at_ms` doc comment still says
  "origination"; the code (GAP-9) now uses local receipt time.

### Notes on prior "verified working" claims

`docs/bug-hunting/README.md` states "BLE 3-way mesh and Wi-Fi Direct message
delivery confirmed live across three physical phones … end-to-end send + receive,
over `adb logcat` on both ends." This was a **single session** on 3 devices. The
volume of subsequently-needed "confirmed live" fixes (HW-1..21, FFI-1..19,
BLE-1/2, AND-RT-*) on the same code paths, several stacked on one root cause,
indicates the delivery path is fragile and timing-dependent, consistent with the
operator's "sometimes works." This tracker does not treat any prior claim as
durable; every finding is re-verified from the current build per the loop's prime
directive.

### Follow-up — framework / internet-research pass (same day)

Per operator direction, ran a web-research pass on (a) the best multi-device
hardware test methodology and (b) deeper mechanism research on the findings.
Searches + page fetches on: Mobly / mobly-snippet-lib / mobly-bundled-snippets /
mobly-bluetooth-ref-validation (AOSP's own BT/Wi-Fi test framework); Android BLE
GATT-133; MTU negotiation timing; write-with vs write-without-response; scan
throttle (AOSP `ScanManager`, 5/30 s); btsnoop HCI logging + Wireshark; Android
14/15 FGS types + background-start restrictions; Wi-Fi Direct GO negotiation;
DTN/mesh field-test methodology (MeshTest testbed, controlled mobility).

**Loop spec updated:**
- Phase R rewritten to be internet-research-first — ≥3–5 searches per finding,
  read AOSP source, cross-check ≥2 sources, cite everything (§1 + §3).
- New **§4 — Test methodology & automation framework**: replaces ad-hoc
  `adb`/manual-tap with **Mobly + Mobly Snippet Lib** host-driven multi-device
  automation (`IrisTestSnippet` APK + `iris_bench` Python suite), a per-test
  evidence pipeline (`btsnoop` HCI logs, time-aligned `logcat`, `dumpsys
  bluetooth_manager` / `dumpsys wifip2p`, `meshSnapshot` JSON), a 4-layer test
  pyramid (sim fault-injection → snippet unit → Mobly 2-phone → manual scenario),
  and a fixed physical bench rig spec. Sections renumbered (§4→§5, §5→§6, §6→§7).

**16 new findings added — Tier 9 in `hardware_problems.md` (HV-65..HV-80):**
- HV-65: GATT writes use `WRITE_TYPE_DEFAULT` (with-response) though the char
  supports `WRITE_NO_RESPONSE` — 4–6× slower, every fragment an ACK round-trip.
- HV-66: no large MTU requested on connect; `setMtu` fire-and-forget; first
  message ships at 23 B; Samsung 9/10 needs a ~600 ms pre-`requestMtu` delay.
- HV-67: no per-connection GATT operation queue — connect/discover/requestMtu/
  write race across threads; the stack is single-threaded, all libs serialize.
- HV-68: stale GATT service cache after a peer restarts — no `refreshDeviceCache`.
- HV-69: scan re-armed every discovery pass — fights the AOSP 5/30 s throttle;
  adopt one long window + ~25 s overlap re-arm + opportunistic baseline.
- HV-70: Android 15 blocks background-initiated FGS starts —
  `IrisBleService.start` from the ViewModel / WorkManager may silently fail.
- HV-71: verify discovery works with device Location **off** (manifest declares
  `neverForLocation` *and* `ACCESS_FINE_LOCATION`).
- HV-72: Wi-Fi Aware NAN session killed by another app / Wi-Fi toggle — recovery
  untested.
- HV-73: clock skew between phones breaks TTL / beacon freshness / replay
  high-water — all use local wall clock.
- HV-74: Wi-Fi Direct TCP socket has no keepalive / heartbeat / idle timeout —
  a half-open socket hangs `readLoop` forever and `p2p_send` keeps "succeeding".
- **HV-75 (elevated, Critical):** the live relay path does **not** call
  `flood.rs` / `NeighborTable` — `mod.rs:21` "true flooding lands in WP-4",
  `:1011`/`:1945` "the routing module … which the live path does not call". The
  Android engine builds no `NeighborTable` and no routing engine. **Multi-hop
  cannot succeed unless the relay node already has a direct link to the final
  recipient.** Flood/PRoPHET aren't just untested — they're not wired to the
  code that runs on the phone. This is the real root of "hopping never works."
- HV-76: `REASSEMBLY_TTL` (sim-era value) vs real serial-fragment wall-time — a
  slow multi-fragment message is silently evicted and lost.
- HV-77: duplicate / out-of-order fragment tolerance on real radios unverified
  (sim delivers in-order, exactly-once).
- HV-78: `dumpsys` + `btsnoop` are the real debugging surface and aren't in a
  repeatable workflow — folds into HV-2's harness.
- HV-79: "courier loop" — a written, timed, repeatable controlled-mobility
  procedure for PRoPHET/SCF (feeds HV-37).
- HV-80: advertising mode / tx power / scan mode all pinned to the low-power
  minimum — discovery is slow and short-range by construction; needs an adaptive
  foreground/background profile.

**Also noted:** HV-42/43 — the missing Android internet/LAN path is *partly a
deliberate v1 scope cut* (manifest: SDK-37 `ACCESS_LOCAL_NETWORK` cliff avoided,
ANDROID_DESIGN.md §8). An outbound client→gateway path does not hit that cliff and
should be reconsidered for v1; LAN P2P (HV-43) is the deferred piece.

### Session 00 closeout

- Findings advanced: 0 → tracked (**81** recorded, all `⬜` — 65 initial + 16
  from the framework/research pass).
- 🟢 count: 0 → 0.
- Blockers opened: HV-35, HV-36, HV-37, HV-39, HV-53, HV-75, HV-79 (🔒 need a
  3rd phone / NAN hardware / mobility); HV-44 (🔒 needs a server + phones on
  separate networks).
- Next session (Session 01): **Tier 0**. Order: HV-1 (make
  `cargo test -p iris-android` compile + wire into CI) → HV-2 (stand up the Mobly
  `iris_bench` harness + `IrisTestSnippet` APK + evidence pipeline per loop §4) →
  HV-3 (`/diag` + `snapshot()` FFI) → HV-4 (sim fault-injection model) → HV-78
  (fold `dumpsys`/`btsnoop` capture into the harness). Gate to Tier 1: a trivial
  "P1 sends, P2 receives" `iris_bench` test passes 10/10 with full evidence
  capture. Hardware needed only for the final harness smoke.
- Suggested parallel track: HV-54 + HV-55 (composer height + send button) — one
  small PR, no transport code, immediately improves the operator's daily
  experience. Can be done by a second contributor any time.

---

## Session 01 — 2026-09-01 — Tier 0 start (HV-1)

**Devices:** P1 = V2205 (vivo, `10BCA20F4M000BB`) · P2 = vivo 2004
(`b2fbcd39`) — both `adb`-reachable, **not yet used this session** (HV-1 is a
host-build finding, no radio component).
**Build under test:** `fc720d5` (main)
**Baseline captured:**
- `cargo build --workspace --all-features` = **FAIL**, pre-existing and unrelated:
  `iris-storage` `lib.rs` `connect_pg` — `if`/`else` arms return
  `RustlsStream<Socket>` vs `NoTlsStream` (E0308). Fails identically on a clean
  `fc720d5` with the working tree stashed. Logged as a new candidate (HV-81
  below); out of HV-1 scope.
- `cargo test -p iris-android` = **FAIL to compile** (the HV-1 defect itself —
  5× `IrisEngine::new` call sites in the test module pass 4 args, ctor takes 5).
- `cargo test -p iris-core` = 762/764; **2 pre-existing failures** unrelated to
  this crate: `routing::tests::rout25_cold_start_sprays_instead_of_flooding` and
  `routing::opportunistic::tests::rout25_spray_fallback_fires_when_cold_start`
  (both land in `iris-core/src/routing/`, last touched by `5ffffb0` /
  `ca02ad6` "ROUT-21/23/25/26"; confirmed failing on stashed `fc720d5`). Logged
  as HV-82 below.
**Session goal:** Tier 0, HV-1.

---

### HV-1 — The Android engine's own unit tests do not compile against the real constructor

**Phase R — Research note** (codebase-only; `HW gate: none` per the tracker, so
no RF/AOSP research — the mechanism is entirely in-tree)

- **Mechanism.** `bde5c8e` ("AN-1 — replace DevCryptoProvider with real
  AndroidCryptoProvider", 2026-08-30) added a 5th parameter
  `signer: Arc<dyn FfiCryptoSigner>` to `IrisEngine::new`
  (`crates/iris-android/src/engine.rs:86`) and swapped the engine's
  `CryptoProvider` from `DevCryptoProvider` to `AndroidCryptoProvider`. It
  updated the Kotlin caller (`IrisCoreModule.kt`) but **not** the five
  `IrisEngine::new(...)` call sites in `#[cfg(test)] mod tests` (lines ~572, 597,
  635, 644, 745 at HEAD). `cargo build --workspace` does not compile test code,
  and nothing in CI names this crate explicitly, so `cargo nextest run
  --workspace` was the only thing exercising it — and that leg has been red (or
  silently skipped in local dev-loops) since 2026-08-30. Net: the one test that
  proves an end-to-end send through the FFI bridge
  (`round_trip_delivers_over_shared_mesh`) has not run for ~2 days of engine
  work.
- **Why the current code is wrong.** Two layers:
  1. The test module cannot name a `dyn FfiCryptoSigner` — there is no test
     double for it anywhere in the crate (`grep FfiCryptoSigner … | grep -i
     test|sim|stub|mock|fake` → nothing).
  2. Even with a signer stub, `round_trip_delivers_over_shared_mesh` uses
     `node_a = [1u8; 32]` / `node_b = [2u8; 32]` — **not** real Ed25519 keys —
     so `AndroidCryptoProvider::verify` (strict `verify_strict` against
     `envelope.sender_id`) and `decrypt` (X25519 DH against a *randomly
     generated, non-injectable* `static_x25519`) can never succeed in a
     two-engine test. Making that test pass under the real provider needs
     AN-6 (inject/derive the X25519 static key), which the commit itself marks
     **blocked**.
- **How this is solved elsewhere.** `iris-core`'s own engine tests
  (`message_engine/mod.rs:1900+`) keep using `DevCryptoProvider` and, since
  PRY-33/RED-0002 (`seal_outbound` fails closed on a missing recipient key),
  register a fake key: `dir.insert(BOB, [0xBB; 32]); engine.set_key_directory(…)`.
  `DevCryptoProvider`'s encrypt/decrypt are pass-through, so the key *value* is
  irrelevant — only its presence in the directory matters. This is the
  established in-repo pattern for "exercise the plumbing, not the crypto."
- **IRIS constraints.** The FFI surface must not change
  (`#[uniffi::export]`/`#[uniffi::constructor]` on `new`); the production path
  must still hard-wire `AndroidCryptoProvider` (no way to inject an inert
  provider from Kotlin). `Arc<dyn CryptoProvider>` is not a uniffi type, so any
  crypto-parameterised constructor must live **outside** the `#[uniffi::export]
  impl` block.
- **Research outcome.** Extract the body of `new` into a private
  `IrisEngine::build(ble, aware, direct, node_id, crypto: Arc<dyn CryptoProvider>)`
  in a plain (non-exported) `impl` block; `pub fn new` keeps its signature and
  calls `build` with `AndroidCryptoProvider`. Add a `#[cfg(test)]`
  `IrisEngine::new_for_test(…)` that calls `build` with `DevCryptoProvider`, plus
  a `#[cfg(test)]` `register_test_key` helper mirroring the iris-core pattern.
  The four registration/lifecycle tests are unchanged in intent; the round-trip
  test gains one line (`a_engine.register_test_key(node_b, [0xBB; 32])`) and
  keeps testing exactly what it tested pre-`bde5c8e` (bridge → transport →
  `SimMeshCoordinator` → inbox), not the crypto backend. Rejected: (a) a real
  Ed25519/X25519 `SimSigner` + real-key node ids — blocked on AN-6, far larger
  than HV-1's "make the test run" goal; (b) `#[ignore]` on the round-trip test —
  defeats the purpose of the finding.

**Phase D — Implementation**

- Files changed:
  - `crates/iris-android/src/engine.rs` — `use …crypto::CryptoProvider`; split
    `new` → `new` (FFI, wires `AndroidCryptoProvider`) + private `build(…, crypto:
    Arc<dyn CryptoProvider>)` in a new plain `impl` block; re-open
    `#[uniffi::export] impl` for the rest of the surface; new `#[cfg(test)] impl`
    with `new_for_test` + `register_test_key`; 5 test call sites →
    `new_for_test`; round-trip test registers B's key before send.
  - `.github/workflows/ci.yml` — explicit named `cargo test -p iris-android`
    step after the doc-tests step (legibility; the `nextest --workspace` leg
    already covered it but the break was invisible in the summary).
- Sim fault-injection test added: none — HV-1 is test-integrity, not a failure
  mode. The fault model is HV-4.
- `cargo test -p iris-android`: **6/6 pass** (was: does not compile).
  ```
  test engine::tests::engine_registers_three_transports_over_bridges ... ok
  test engine::tests::start_all_brings_transports_up_over_bridges ... ok
  test engine::tests::round_trip_delivers_over_shared_mesh ... ok
  test engine::tests::engine_owns_a_live_runtime ... ok
  test bridge::tests::ble4_draining_one_handle_does_not_destroy_another_peers_frames ... ok
  test ffi::ble_adapter::tests::projection_trait_drives_sync_ops ... ok
  test result: ok. 6 passed; 0 failed
  ```
- `cargo build -p iris-android` / `cargo clippy -p iris-android --all-targets`:
  pass (2 clippy warnings in `bridge.rs` are pre-existing `manual_str_repeat` /
  `manual_repeat_n`, not touched here).

**Phase T — Hardware test**

- n/a. HV-1 has no radio component (`HW gate: none`). Per loop §0 this cannot be
  marked `🟢` (that status is reserved for on-phone evidence); it is marked
  `✅ Fixed` with the CI/`cargo test` evidence above. It unblocks the sim
  round-trip that every Tier 1+ BLE/Wi-Fi-Direct finding will regression-check
  against.

**Phase C — Close**

- Commit: 9a0400b — `fix(hw/test-integrity): HV-1 — Android engine tests compile + run against the 5-arg constructor`
- Tracker: HV-1 → `✅ Fixed` (Tier 0). Progress table Tier 0: `⬜ 6 → ⬜ 5`,
  `✅ 0 → 1`.
- Docs corrected: none overclaimed HV-1 specifically. `ANDROID_BUILD_STATUS.md`
  is the natural home for "iris-android tests now gate in CI" — deferred to the
  HV-2 harness commit which rewrites that doc's CI section wholesale.

---

### HV-5 — `received_at_ms` doc comment still says "origination", code says local receipt

**Phase R.** GAP-9 changed `FfiIncomingMessage.received_at_ms` to
`unix_now().saturating_mul(1000)` (local receipt time) at `engine.rs:351` — the
sender's self-declared origination time is peer-controlled and unsafe as an inbox
sort key (a peer could pin or bury its own messages). The `///` doc on the field
(`engine.rs:50`) was never updated and still read "Unix epoch milliseconds at
origination", directly contradicting the code and the `GAP-9` comment 300 lines
below it. Kotlin `MeshRepository.toUi` consumes it as `receivedAtMs` for sort
order. No `originated_at_ms` field exists, so the UI cannot distinguish a freshly
relayed hour-old message from a new one — noted in the corrected doc as a known
limitation rather than fixed (adding a field is a UI-driven change, not a
doc-drift fix).

**Phase D.** `crates/iris-android/src/engine.rs` — rewrote the field doc comment
to state local-receipt semantics, cite GAP-9, name the Kotlin consumer, and flag
the missing-origination-time limitation. Comment-only; `cargo build -p
iris-android` passes.

**Phase T.** n/a — documentation, no radio component.

**Phase C.** Commit 5fdaeeb. Tracker: HV-5 → `✅ Fixed`. Tier 0 counts
`✅ 1 → 2`, `⬜ 5 → 4`.

---

### New candidates spotted (not fixed)

- **HV-81** — `iris-storage` does not build under `--all-features`:
  `connect_pg` `if tls { pg.connect(make_tls_connector()?) } else {
  pg.connect(NoTls) }` — the two arms have incompatible `Connection<_, S>` types
  (E0308). Needs `.map(|(c, _)| c)` normalisation or boxing the stream. Blocks
  `cargo build --workspace --all-features` (a loop Phase-D gate). Pre-existing at
  `fc720d5`.
- **HV-82** — `iris-core` ROUT-25 cold-start spray: two tests
  (`rout25_cold_start_sprays_instead_of_flooding`,
  `rout25_spray_fallback_fires_when_cold_start`) fail at `fc720d5`. The
  cold-start-spray-instead-of-flood behaviour added in `5ffffb0` regressed (or
  the tests were committed red). Relevant to Tier 4 (flood/PRoPHET) research —
  the live relay path's routing is already the subject of HV-75.

### Session 01 closeout

- Findings advanced: HV-1 `⬜ → ✅ Fixed` (CI-verified; no HW gate); HV-5
  `⬜ → ✅ Fixed` (doc/comment drift, no HW gate).
- 🟢 count: 0 → 0 (neither finding has a hardware component).
- ✅ count: 0 → 2. Tier 0 `⬜` 6 → 4.
- Blockers opened: none new in the HV-1..HV-80 set. Two pre-existing repo
  breakages logged as HV-81 (build, `--all-features`) and HV-82 (iris-core
  routing tests) — both predate this session and are outside Appendix B's
  Android scope, but HV-81 blocks the Phase-D `--all-features` build gate for
  every subsequent finding and should be fixed first.
- Hardware: both phones enrolled and `adb`-reachable (P1 = V2205 `10BCA20F4M000BB`,
  P2 = vivo 2004 `b2fbcd39`); no radio test run — neither finding this session
  has a radio component.
- Next session should pick up: **HV-2** — the Mobly `iris_bench` harness +
  `IrisTestSnippet` APK + evidence pipeline (loop §4). It is the large Tier-0
  deliverable and the gate to Tier 1, and warrants its own focused session with
  the phones on the bench. Clear **HV-81** first so `cargo build --workspace
  --all-features` is green for the rest of the tier. HV-3 (`/diag` +
  `snapshot()` FFI) and HV-4 (sim fault model) follow.

---

## Session 02 — 2026-09-01 — Clear the blockers (HV-81, HV-82), then Tier 0

**Devices:** P1 = V2205 (`10BCA20F4M000BB`) · P2 = vivo 2004 (`b2fbcd39`) —
enrolled, not used yet (blocker work + Tier-0 instrumentation are host-only).
**Build under test:** `2fbad16` → HEAD (main)
**Session goal:** HV-81, HV-82 (unblock the Phase-D build/test gates), then
carry on Tier 0 (HV-2 / HV-3 / HV-4) until a hardware step is required.

---

### HV-81 — `iris-storage` does not build (TLS/NoTls `Connection` type mismatch)

**Phase R.** TAK-6 added `SslMode` and made `establish()` pick rustls vs `NoTls`
at runtime:
```rust
let (client, connection) = if *ssl_mode == SslMode::Require {
    pg.connect(make_tls_connector()?) ...      // Connection<Socket, RustlsStream<Socket>>
} else {
    pg.connect(tokio_postgres::NoTls) ...       // Connection<Socket, NoTlsStream>
};
```
The two arms of the `if` produce different concrete `Connection<S>` types, so the
expression has no single type — `E0308`, arm 2 "expected `RustlsStream<Socket>`,
found `NoTlsStream`". `cargo build -p iris-storage` fails on a clean `fc720d5`
(the tracker note said "`--all-features`" but it fails on default features too —
there is no `cfg` gate on the TLS path). A sibling break: the TAK-6 `ssl_mode`
field was never added to the `PgStorageConfig` literal in
`seal.rs::config_debug_redacts_password`, so `cargo test -p iris-storage` also
fails to compile (`E0063 missing field ssl_mode`).

`tokio_postgres`'s own docs spawn the driver per connection immediately after
`connect()`; the connection value is only ever `.await`ed on a spawned task and
never returned to callers. So there is no need for the `if`/`else` to yield the
`Connection` at all — each arm can spawn its own driver and yield only `Client`.

**Phase D.** `crates/iris-storage/src/pg.rs`:
- New generic helper
  `fn spawn_pg_driver<S>(connection: tokio_postgres::Connection<tokio_postgres::Socket, S>)
  where S: AsyncRead + AsyncWrite + Unpin + Send + 'static` — the shared driver
  spawn, monomorphised per stream type.
- `establish()`: the `if`/`else` now binds `(client, connection)` inside each
  arm, calls `spawn_pg_driver(connection)`, and yields `client`. Behaviour is
  identical (driver task spawned, `tracing::debug!` on driver exit).

`crates/iris-storage/src/seal.rs`: add `ssl_mode: SslMode::Require` to the test
config literal; import `SslMode`.

- `cargo build -p iris-storage`: **pass** (was E0308).
- `cargo build --workspace --all-features`: **pass** (1m03s) — the Phase-D build
  gate is green again.
- `cargo test -p iris-storage`: **21 + 1 + 2/16-ignored pass, 0 fail** (was:
  does not compile).

**Phase T.** n/a — no radio component; needs a live Postgres for the `#[ignore]`
integration tests, which is not this pass's concern.

**Phase C.** Commit e044bda. Tracker: new **HV-81 → `✅ Fixed`** under Tier 0.

---

### HV-82 — `iris-core` ROUT-25 cold-start-spray tests fail at HEAD

**Phase R.** Two tests panic on a clean `fc720d5`:
- `routing::opportunistic::tests::rout25_spray_fallback_fires_when_cold_start`
  — `left: NoAdvantage`, `right: ForwardTo{…Spray}`.
- `routing::tests::rout25_cold_start_sprays_instead_of_flooding` — "expected a
  spray fallback Forward, got Flood{…}".

Both drive a **P4** message. `git log -- routing/`: `5ffffb0` (ROUT-25) added
`spray_fallback` + these tests when `l_for_priority(P4, _)` still returned 3
(sprayable). Then `ca02ad6` (ROUT-23, 2026-08-31) rewrote
`SprayBudget::l_for_priority` to the `OPPORTUNISTIC_ROUTING.md §3.1` table —
**P4–P7 → L=1 ("direct-only"; old value caused 3× overhead on bulk traffic)**.
`SprayBudget::new(1)` is immediately in the wait phase (`handoff()` returns
`None` when `remaining <= 1`), so `spray_fallback` for P4 now correctly returns
`NoAdvantage` and `decide()` falls through to Flood. ROUT-23 (later, spec-cited)
is authoritative; the ROUT-25 tests were left asserting the pre-ROUT-23
behaviour. Stale test, not a code regression — loop rule "if a sim test
contradicts reality, fix the test to model reality" applies. Sim-only today
(HV-75: the live Android relay path never calls the routing module) but must be
correct before Tier 4.

**Phase D.**
- `routing/opportunistic.rs`: `rout25_spray_fallback_fires_when_cold_start` →
  `MessagePriority::P2` (L=3, spray-eligible); new
  `rout25_spray_fallback_direct_only_priority_does_not_spray` pins P4 → `NoAdvantage`.
- `routing/mod.rs`: `rout25_cold_start_sprays_instead_of_flooding` `decide(…)` →
  `P2`; new `rout25_direct_only_priority_cold_start_still_floods` pins P4 → `Flood`.
- `cargo test -p iris-core rout25`: **7/7** (was 3/5 + 2 failing).
- `cargo test -p iris-core -p iris-android`: lib+bin **all green**. One
  *pre-existing, unrelated* integration failure remains outside scope —
  `sysval_mesh_integration::p0_multipath_includes_satellite_emergency_only`
  (`:223`); confirmed failing on stashed `fc720d5`, logged as HV-83.

**Phase T.** n/a — sim routing logic, no radio component.

**Phase C.** Commit 6c0a755. Tracker: HV-82 → `✅ Fixed`.

---

### New candidates spotted (Session 02)

- **HV-83** — `sysval_mesh_integration::p0_multipath_includes_satellite_emergency_only`
  (`crates/iris-core/tests/sysval_mesh_integration.rs:223`) panics on a clean
  `fc720d5`. Pre-existing; found running the HV-82 test gate. Likely
  transport-selection / `RadioConflictGroup` fallout (cf. HV-46). Added to the
  Tier-0 tracker as `⬜`.

### Session 02 closeout

- Findings advanced: **HV-81** `⬜ → ✅ Fixed` (`e044bda`) — the build gate;
  **HV-82** `⬜ → ✅ Fixed` (`6c0a755`) — the ROUT-25 test gate. Both host-only,
  no HW component, so neither is `🟢`.
- 🟢 count: 0 → 0. ✅ count: 2 → 4 (Session 01: HV-1, HV-5; Session 02: HV-81,
  HV-82). Tier 0 now 9 findings: 4 ✅, 5 ⬜ (HV-2, HV-3, HV-4, HV-6, HV-83).
- Gates restored: `cargo build --workspace --all-features` ✅;
  `cargo test -p iris-storage` ✅; `cargo test -p iris-core` — all lib tests
  green **except** the pre-existing HV-83 integration failure;
  `cargo test -p iris-android` 6/6.
- Blockers opened: none. HV-83 logged as a new pre-existing `⬜` (does not block
  the Android/BLE work; blocks a fully-green `cargo test -p iris-core`).
- Hardware: not used — all four findings to date are host-only.
- **Stopped here — the rest of Tier 0 is human-in-the-loop.** Remaining:
  - **HV-2** — the Mobly `iris_bench` harness + `IrisTestSnippet` APK + evidence
    pipeline. This is the gate to Tier 1 and is inherently a bench activity:
    it drives two physical phones from the laptop, and building/installing the
    snippet APK needs the gradle + NDK + `cargo ndk` toolchain exercised with
    the operator present (Android build history in
    `docs/testing/ANDROID_BUILD_STATUS.md` shows this path has been fragile).
    There is **no `gradlew` wrapper script** in `android/` — only
    `gradle/wrapper/gradle-wrapper.jar`; the invocation is
    `java -cp gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain <tasks>`
    (SDK at `C:/Users/TK/AppData/Local/Android/Sdk`, `cargo-ndk 4.1.2`,
    JDK 25 present). Confirm `assembleDebug` builds before HV-2 starts.
  - **HV-3** — `/diag` + `snapshot()` FFI (transports, per-transport state,
    live links + MTU, discovered peers, scan-backoff, last-N send outcomes) +
    a rolling event ring over FFI + consistent `iris.*` tags. Large; touches
    iris-core (to expose state not currently queryable), iris-android FFI, and
    Kotlin (`CommandExecutor`/`CommandRegistry` `/diag`). The loop wants it
    "verified on device", and its struct **is** HV-2's `meshSnapshot()` — build
    the two together.
  - **HV-4** — injectable fault model on the sim adapters (mid-stream MTU
    change, unsolicited disconnect, `onScanFailed` after N restarts, `BUSY` on
    first K calls, GO/GO tie) + `sysval_*` recovery scenarios. `SimulatedTransport`
    already models loss/latency/MTU/connect-failure/bandwidth/order; the new
    modes belong partly there and partly on the iris-android `SimBle`/`SimDirect`/
    `SimAware`. Best built against the specific Tier-1 findings it must
    reproduce (HV-7 fragment failure, HV-14 scan backoff, HV-19 GO/GO).
  - **HV-78** — fold `dumpsys`/`btsnoop` capture into the HV-2 harness.
- Suggested when the operator is next at the bench: (1) confirm
  `assembleDebug` + `installDebug` on both phones; (2) decide HV-3's FFI
  struct shape together (it locks HV-2's RPC surface); (3) then the agent
  builds HV-3 + HV-2 and the operator runs the 2-phone smoke.

---

## Session 03 — 2026-09-01 — Get the APK building + on both phones (HV-84)

**Devices:** P1 = V2205 (`10BCA20F4M000BB`) · P2 = vivo 2004 (`b2fbcd39`) — both
on USB, `adb`-reachable. **Used this session** (APK install + engine-start smoke).
**Build under test:** `ab09ffe` → HEAD (main)
**Session goal:** stand up the Android build toolchain and get a debug APK
running on both phones — the hard prerequisite for HV-2 (the Mobly harness) and
every hardware finding after it.

---

### HV-84 — The debug APK has not built since 2026-08-30

**Phase R.**
- `./gradlew :app:assembleDebug` (via
  `java -cp gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain`)
  fails at `:app:kspDebugKotlin`:
  ```
  e: [ksp] ModuleProcessingStep was unable to process 'iriscore.di.IrisCoreModule'
     because 'FfiCryptoSigner' could not be resolved.
  e: [ksp] BindingMethodProcessingStep ... 'provideFfiCryptoSigner(...)' ...
  > KSP failed with exit code: PROCESSING_ERROR
  ```
- `bde5c8e` (AN-1, 2026-08-30) added the `FfiCryptoSigner` uniffi trait
  (`crates/iris-android/src/ffi/crypto_signer.rs`) and, on the Kotlin side,
  `import iriscode.FfiCryptoSigner` + `provideFfiCryptoSigner` in
  `IrisCoreModule.kt`. It did not regenerate the committed uniffi bindings
  (`kotlin/src/main/kotlin/iriscore/uniffi/iriscode/iriscode.kt` — no
  `FfiCryptoSigner` symbol, `grep -c` = 0) and did not add the facade alias to
  `kotlin/src/main/kotlin/iriscode/api.kt`. Hilt's KSP processor then can't
  resolve the type referenced by every `IrisCoreModule` provider. Same failure
  family as HV-1 — one commit, several un-propagated edits.
- Toolchain gap: the machine had only **JDK 25**; Gradle 8.9 aborts on it
  (opaque `* What went wrong: 25.0.2`). AGP 8.7.3 + Gradle 8.9 want JDK 17–21.
- `Cargo.lock` was stale: missing the `rustls` / `ring` / `aws-lc-rs` / `bcder`
  tree that `iris-storage`'s `tokio-postgres-rustls` path resolves — every
  `cargo build` silently rewrote it, and a `--locked` CI build would fail.

**Phase D.**
- Installed **Temurin JDK 21** (`winget install EclipseAdoptium.Temurin.21.JDK`
  → `C:/Program Files/Eclipse Adoptium/jdk-21.0.12.101-hotspot`). Gradle invoked
  with `-Dorg.gradle.java.home=<jdk21>` + `JAVA_HOME`/`PATH` set;
  `ANDROID_HOME=C:/Users/TK/AppData/Local/Android/Sdk`.
- `cargo build -p iris-android` (host) → `target/debug/iriscode.dll`;
  `uniffi-bindgen 0.31.2 generate --library target/debug/iriscode.dll --language
  kotlin` → replaced `iriscode.kt` (6854 → 7220 lines; `FfiCryptoSigner` now
  present, 27 refs).
- `kotlin/src/main/kotlin/iriscode/api.kt`: added
  `typealias FfiCryptoSigner = uniffi.iriscode.FfiCryptoSigner` next to the
  other foreign-trait aliases.
- `cargo ndk -t arm64-v8a -t x86_64 -t armeabi-v7a -o android/app/src/main/jniLibs
  build --release -p iris-android` — refreshed all three `libiriscode.so`.
- Committed the regenerated `Cargo.lock`.
- `:app:assembleDebug` → **BUILD SUCCESSFUL in 58s** (deprecation warnings only).

**Phase T — hardware.**
- `adb -s <serial> install -r -d app-debug.apk` → `Success` on both phones.
- `adb shell monkey -p org.iris.mesh -c android.intent.category.LAUNCHER 1` on
  both; `adb logcat` (tag `iriscore`):
  ```
  P1  I ActivityTaskManager: Displayed org.iris.mesh/iriscore.ui.MainActivity ... +2s446ms
  P1  W iriscore: iriscode::engine: transport failed to start; trying the others
        event="engine.start_transport_failed" transport="wifi-aware-0"
        error=... wifiaware.start: not supported on this device
  P1  D iriscore: iris_core::discovery: discovery pass complete ... transport=ble-android peers_seen=0
  P2  W iriscore: iriscode::engine: mesh started with a subset of transports
        event="engine.start_all_partial" started=["ble-android"]
        failed=["wifi-aware-0: ... not supported on this device",
                "wifi-direct-0: ... WifiP2p action failed reason=2"]
  ```
- **Pass criteria met:** APK builds, installs on both phones, app launches
  without `UnsatisfiedLinkError`, the Rust engine initialises, its `tracing`
  reaches logcat (HW-3), and **BLE `start_advertising` + discovery loop come up
  on both phones**. No crash on either device.
- Not verified here (later tiers): peer discovery (`peers_seen=0` — permissions
  not yet granted, and this is Tier 1), Wi-Fi Aware (no NAN hardware — Tier 7,
  deferred), Wi-Fi Direct (P2 `reason=2` BUSY on cold start — HV-23).

**Phase C.**
- Commit 3daf0da.
- Tracker: **HV-84 → 🟢** (build + engine-start verified on 2 phones with logcat).
  Tier 0: 10 findings — 1 🟢, 4 ✅, 5 ⬜.
- `docs/testing/ANDROID_BUILD_STATUS.md` is stale (66-error state from a much
  earlier attempt) — flagged for correction in the HV-2 commit that rewrites
  that doc's CI/build section.

---

### HV-85 — Android JVM unit-test leg does not compile

**Phase R.** `:app:compileDebugUnitTestKotlin` fails on a clean `fc720d5`:
`AdapterLifecycleTest.kt:88` etc. — `Cannot infer type` / `No value passed for
parameter 'create'`. `9431aca` (FFI-12, 2026-08-27) added `dispose` as
`SessionGate`'s 2nd ctor param (defaulted); Kotlin routes a trailing lambda to
the last param, so `SessionGate { … }` binds it to `dispose`. Production sites
all use `SessionGate(create = { … })`.

**Phase D.** 5 test call sites → `SessionGate(create = { … })`.
`:app:testDebugUnitTest` → **77/77** (76 pre-existing + HV-3's new one).

**Phase C.** Commit 13c5e2e. Tracker **HV-85 → ✅**.

---

### HV-3 — /diag mesh snapshot over FFI

**Phase R.** Field debugging today: `adb logcat | grep` across `IrisBle` /
`IrisBleDiag` / `IrisWifiDirectDiag` / `iriscode::engine` / `iris_core::*` — no
single "what is the mesh doing now". The Rust side already exposes everything
needed piecemeal: `TransportManager::{list,get}` + `Transport::{state,
cost_snapshot,display_name}`, `DiscoveryManager::neighbors()` →
`NeighborTable::neighbor_summaries()` (peer + per-transport links),
`MessageEngine::metrics()` → `MetricsSnapshot`. HW-3's logcat bridge is already
installed. So the fix is an aggregation + FFI record, not new transport surgery.
The deeper per-link MTU / scan-backoff state is *not* on the `Transport` trait —
that needs a `Transport::diagnostics()` method touching every impl → deferred to
HV-86 along with the `iris.*` tag rename and the event ring.

**Phase D.**
- `crates/iris-android/src/engine.rs`: `FfiMeshSnapshot` + `FfiTransportDiag` +
  `FfiNeighborDiag` + `FfiMessageMetrics` records; `IrisEngine::snapshot()`
  (`block_on` for the async manager/neighbour reads) which also emits
  `tracing::info!(event = "iris.diag", …)`.
- Regenerated `kotlin/…/uniffi/iriscode/iriscode.kt` (`uniffi-bindgen 0.31.2
  generate --library target/debug/iriscode.dll --language kotlin`); 4 new
  `typealias` in `kotlin/…/iriscode/api.kt`.
- `MeshRepository.snapshot()` (blocking FFI, off-main); `/diag` in
  `CommandRegistry` (aliases `diagnostic`, `snapshot`) + `CommandExecutor`
  (`→ System("DIAG")`); `MeshViewModel` resolves DIAG on `Dispatchers.IO` and
  renders transports / neighbours / counters.
- `crates/iris-desktop/ui/commands.js`: added the `diag` entry + `case "diag"`
  so `DesignTokenParityTest` (`command set matches the desktop mirror`) passes.
- Tests: `snapshot_reports_transports_and_identity` (Rust, `cargo test -p
  iris-android` → 8/8); `diag resolves to a DIAG system block` (Kotlin).

**Phase T — hardware.** Reinstalled on both phones, granted BLE/Wi-Fi/location
perms, launched, typed `/diag` in the console (`adb shell input`):
```
P1 (72ddbbb8…) I iriscore: iriscode::engine: mesh snapshot  event="iris.diag"
   transports=[FfiTransportDiag { id: "ble-android", state: "Available",
     estimated_battery_ma: 3.0, bandwidth_available_bps: 1000000, … },
     { id: "wifi-aware-0", state: "Unavailable", … },
     { id: "wifi-direct-0", state: "Degraded", … }]
   neighbors=[]  messages=FfiMessageMetrics { sent: 0, delivered: 0, … }
P2 (3782eb73…) I iriscore: … event="iris.diag" … wifi-direct-0 state: "Available" …
```
Screenshot confirms the `DIAG` block also renders in the console UI (`>
wifi-aware-0  Unavailable  batt=25.0mA …` / `> messages  sent=0 …` /
`STATUS: RUNNING`). The composer overlaps the newest lines — that is **HV-54**,
unchanged here.

**Phase C.** Commit a7daad0. Tracker **HV-3 → 🟢 (partial — see HV-86)**.

### Session 03 closeout
- Findings advanced: **HV-84 🟢** (APK builds + runs on both phones — first in
  days), **HV-3 🟢** (`/diag`/`snapshot()` verified on both phones),
  **HV-85 ✅** (unit-test leg compiles). New follow-ups logged: **HV-86**
  (HV-3's `iris.*` tag rename + event ring — deferred).
- 🟢 count: 0 → 2. ✅ count: 4 → 5. Tier 0: 12 findings — 2 🟢, 5 ✅, 5 ⬜
  (HV-2, HV-4, HV-6, HV-83, HV-86).
- Toolchain now working and documented (JDK 21, gradle-wrapper-jar invocation,
  `cargo ndk` ABIs, uniffi regen procedure) — in this log's HV-84 entry and in
  the agent's memory.
- Blockers: none open. Pre-existing `⬜`: HV-83 (iris-core satellite integration
  test), HV-86 (HV-3 tail).
- **Next: HV-2** — the Mobly `iris_bench` harness + `IrisTestSnippet` APK +
  evidence pipeline (loop §4). Everything it needs is now in place: the APK
  builds, `snapshot()`/`nodeId()`/`sendText()` exist over FFI, and both phones
  run the engine. The snippet APK adds `startMesh`/`meshSnapshot`/`awaitDelivered`
  RPCs (Mobly Snippet Lib) + the Python `iris_bench` suite. Also rewrite the
  stale `docs/testing/ANDROID_BUILD_STATUS.md` in that commit. HV-4 (sim fault
  model) and HV-6 (KPI capture) follow.

---

## Session 04 — 2026-09-01 — HV-4 (sim fault model) + HV-6 (KPI capture)

**Devices:** P1 = V2205 (`10BCA20F4M000BB`) · P2 = vivo 2004 (`b2fbcd39`) — both
on USB. Used for HV-6 / HV-88 (`/stats` + APK-launch smoke).
**Build under test:** HEAD after a `git pull` fast-forwarded `e53f613` -> `3e70a87`
(6 upstream commits from the other bug-hunt loops: ROUT-24/PS-1/PS-3/F-C1,
PRY-11 SOS auth, AN-6 X25519, future-scope, shrey reclassify, integration
report). All Session 01-03 HV work is intact below `e53f613`.
**Baseline captured:** `cargo build --workspace` = ok. `cargo test -p iris-core`
= **FAIL to compile** — upstream added `dp_snapshot` to `CapabilityBundle` and
left one test literal un-updated (fixed as **HV-87**, commit 678dfe3). After
that: iris-core lib 766/766; iris-android 7/7; the one pre-existing failure
`sysval_mesh_integration::p0_multipath_includes_satellite_emergency_only`
(HV-83) still fails, unrelated.

---

### HV-87 — CapabilityBundle test literal missing dp_snapshot

**Phase R/D.** Upstream added `dp_snapshot: Vec<(Vec<u8>, u16)>` to
`CapabilityBundle` (PRoPHET DP exchanged over the discovery handshake) but the
literal in `neighbor_table.rs::capabilities_recorded_and_retained` was not
updated -> E0063, `cargo test -p iris-core` won't compile. Added
`dp_snapshot: Vec::new()`. iris-core lib 766/766.
**Phase C.** Commit 678dfe3. Tracker HV-87 -> Fixed.

---

### HV-4 — The simulated adapters model success, not RF

**Phase R.** `SimulatedTransport` (`crates/iris-core/src/transport/simulated.rs`)
already models packet loss, latency + spread, a static MTU, `connect_failure_rate`,
a token bucket and optional ordering (RF-45-a..d). It does NOT model the
failure modes the Tier-1..3 findings are about: a link that dies mid-stream with
no callback (HV-7 `close_peer`, HV-27 silently-dead link), a peripheral-role
MTU that never renegotiates up (HV-8), `onScanFailed(SCANNING_TOO_FREQUENTLY)` /
the AOSP `ScanManager` 5-per-30 s throttle (HV-11 / HV-14), and `WifiP2pManager`
`reason=BUSY` during the ~30 s P2P bring-up window (HV-23 — seen live on P2 this
session: `wifi_direct.dns_sd ... WifiP2p action failed reason=2`). The
iris-android `SimBle`/`SimDirect`/`SimAware` are trivial `#[cfg(test)]` stubs
with no state machine; the real fault surface is `SimulatedTransport`, which is
registered in `TransportManager` and drives the engine tests. Determinism
(SIM-001, seeded ChaCha8) must carry over.

**Phase D.** `SimConfig` gains four modes, each disabled at its zero default so
every existing test is unaffected:
- `disconnect_after_sends` — after N successful `send()`s the transport
  `set_state(Degraded)` and the send returns `TransportError::NotConnected`.
- `mtu_shrink_after_sends` + `mtu_after_shrink` — the effective MTU used by the
  size check drops mid-stream.
- `scan_fail_after_calls` — `discover_peers()` returns `TransportError::Busy`
  after the first N.
- `busy_first_n_connects` — `connect()` returns `TransportError::Busy` for the
  first N calls.
Backed by three `AtomicU32` counters on the struct. 5 unit tests in
`simulated.rs` + `crates/iris-core/tests/sysval_fault_injection.rs` (5 scenarios
asserting the observable recovery property). `cargo test -p iris-core` green;
`sysval_fault_injection` 5/5. GO/GO election tie deferred to HV-19.

**Phase T.** n/a — this is the L1 CI layer; no radio component.

**Phase C.** Commit 1ad395d. Tracker HV-4 -> Fixed (no HW gate).

---

### HV-88 — Upstream AN-6 FFI wiring incomplete: APK crashed on launch

**Phase R.** After the pull, `:app:assembleDebug` failed at `:app:kspDebugKotlin`
— `'FfiX25519KeyProvider' could not be resolved` across `IrisCoreModule`. The
AN-6 commit added the `#[uniffi::export(with_foreign)] trait FfiX25519KeyProvider`
+ the `new_with_x25519` constructor + the Kotlin call sites, but not: the
regenerated `iriscode.kt`, the `api.kt` facade alias, or the jniLibs `.so`.
Adding the alias got past KSP; the app then FATAL-crashed on first launch:
```
FATAL EXCEPTION: DefaultDispatcher-worker-2
java.lang.UnsatisfiedLinkError: Error looking up function
'uniffi_iriscode_fn_clone_ffix25519keyprovider': undefined symbol
  at uniffi.iriscode.IrisEngine$Companion.newWithX25519(iriscode.kt:6404)
  at iriscore.di.IrisCoreModule.provideIrisEngine(IrisCoreModule.kt:82)
```
— the regenerated `iriscode.kt` referenced FFI symbols absent from the committed
`.so` (which `cargo ndk` had not rebuilt; `touch crates/iris-android/src/lib.rs`
forced it — cargo's fingerprint had not noticed the pulled source changes).
Third instance of the same pattern (HV-1 `signer`, HV-84 `FfiCryptoSigner`, now
`FfiX25519KeyProvider`).

**Phase D.** `cargo build -p iris-android` -> `uniffi-bindgen 0.31.2 generate
--library target/debug/iriscode.dll --language kotlin` -> `iriscode.kt`;
`typealias FfiX25519KeyProvider = uniffi.iriscode.FfiX25519KeyProvider` in
`api.kt`; `cargo ndk -t arm64-v8a -t x86_64 -t armeabi-v7a ... build --release`
for all three `.so`. (Committed with HV-6 — the same regen carries HV-6's new
`FfiCounter` record.)

**Phase T — hardware.** Both phones launch the app, load the `.so`, and start
the engine (`iriscode::engine start_all_partial started=["ble-android"]`) — the
`UnsatisfiedLinkError` is gone.

**Phase C.** Commit 49f0d99 (with HV-6). Tracker HV-88 -> HW-verified.
Recommended (feeds HV-2): a CI job that runs bindgen + `cargo ndk` +
`:app:assembleDebug` and fails on any diff to the committed `iriscode.kt` /
`.so`.

---

### HV-6 — No field-KPI capture

**Phase R.** `MetricsRegistry::snapshot()` already returns the full `metric::ALL`
counter map (`msg.*`, `route.*`, `scf.*`, `gw.*`, `security.*`), and
`MessageEngine::telemetry()` exposes the live registry — HV-3's `snapshot()`
only surfaced the typed six-field `MetricsSnapshot`. Missing: the raw map on the
FFI + a shell readout + a greppable per-capture log line. The deeper KPIs
(median GATT link lifetime, scan-restart count/hour, group-formation success
rate) need per-link instrumentation that does not exist yet — HV-27 + HV-86;
HV-6 delivers the message/route/security half now.

**Phase D.**
- `engine.rs`: `FfiCounter { name, value }`; `FfiMeshSnapshot.counters:
  Vec<FfiCounter>` from `self.engine.telemetry().snapshot()`, sorted; a second
  `tracing::info!(event = "iris.kpi", ...)` line with the non-zero
  `name=value` list.
- `/stats` command: registry (aliases `kpi`, `metrics`), executor
  (`-> System("STATS")`), `MeshViewModel.resolveStats()` on `Dispatchers.IO`,
  `commands.js` mirror + `case "stats"`.
- Tests: `stats resolves to a STATS system block` (Kotlin);
  `:app:testDebugUnitTest` 78/78; `cargo test -p iris-android` 7/7.

**Phase T — hardware.**
```
P1  I iriscore: iriscode::engine: mesh kpi  event="iris.kpi"  node=72ddbbb8...  counters=
P2  I iriscore: ... event="iris.kpi" ... counters=
```
Screenshot: the `STATS` console block renders — `> counters   all zero` /
`STATUS: RUNNING`. A `/to <P2-id>` + `/send hello-kpi` shows the Android outbox
`QUEUED` chip — the core engine correctly reports zero because `seal_outbound`
rejected the send for a missing recipient X25519 key (PRY-33) and
`MeshRepository` spooled it locally (HV-38). `/stats` still reads all-zero,
which is accurate. A non-zero demonstration needs real two-phone delivery
(Tier 1).

**Phase C.** Commit 49f0d99. Tracker HV-6 -> HW-verified (partial — see HV-27 / HV-86).

### Session 04 closeout
- Findings advanced: **HV-4 ✅** (sim fault model — CI), **HV-6 🟢** (`/stats` +
  `iris.kpi`, both phones), **HV-87 ✅** (upstream `dp_snapshot` test break),
  **HV-88 🟢** (upstream AN-6 left the APK non-building / crash-on-launch —
  regen bindings + `.so`).
- 🟢 count: 2 -> 4. ✅ count: 5 -> 7. **Tier 0: 14 findings — 4 🟢, 7 ✅, 3 ⬜
  (HV-2, HV-83, HV-86).**
- **Gate to Tier 1** now needs only HV-2: `cargo test -p iris-android` in CI
  (HV-1) + the L1 sim fault model (HV-4) + `iris_bench` driving two phones with a
  10/10 "P1 sends, P2 receives" + evidence pipeline.
- Recurring risk (HV-88): three FFI-surface changes in a row (`signer`,
  `FfiCryptoSigner`, `FfiX25519KeyProvider`) each broke the build or runtime
  because nothing regenerates/checks the committed bindings + `.so`. A CI gate
  for this should be part of HV-2.
- Next session: **HV-2** — with the operator, per the earlier plan.

---

## Session 05 — 2026-09-01 — HV-86 (tag unification + transport-event ring)

**Devices:** P1 = V2205 (`10BCA20F4M000BB`) · P2 = vivo 2004 (`b2fbcd39`) — both
on USB. Used for the `/diag` ring + `adb logcat -s iriscore` checks.
**Build under test:** `30946aa` -> HEAD.
**Session goal:** HV-86 (the last non-HV-2 Tier-0 finding); reclassify HV-83.

---

### HV-83 — reclassified

Operator decision: satellite is post-v1 scope, will not ship or be benched in
this pass. The failing test (`sysval_mesh_integration::p0_multipath_includes_
satellite_emergency_only`) asserts P0-multipath behaviour for the satellite
transport — not on the Tier-0 critical path. Marked **🔒 deferred** in the
tracker. Everything the mesh tiers exercise in `cargo test -p iris-core` is
green; only the satellite path fails.

---

### HV-86 — iris.* log-tag unification + the transport-event ring

**Phase R.** Two gaps left after HV-3:
- (a) The Rust `tracing` events already share one logcat tag (`iriscore`) and a
  consistent `event = "area.verb"` field (`discovery.connect_ok`,
  `msg.all_sends_failed`, `engine.start_transport_failed`, …). The **Kotlin
  adapters** do not: 14 `Log.d`/`Log.w` calls under three separate tags
  (`IrisBleDiag`, `IrisWifiDirectDiag`, `IrisBle`), so a bench operator has to
  run four `adb logcat -s` filters and still misses correlation.
- (b) `MetricsRegistry` answers "how many"; nothing answers "what just happened,
  in what order". The whole point of this pass is intermittent-failure
  debugging — at the moment a send fails you need the last N transport events
  (which transport was selected, did a scan just get throttled, did a peer
  drop). No such buffer exists.
  `tracing` already emits everything needed as taxonomy events; the minimal
  capture is a `Layer` that siphons those into a bounded ring — **zero edits to
  the ~40 existing log sites**, and it cannot record payloads or full ids
  because the events themselves can't (P1/P2/P3).

**Phase D.**
- **`crates/iris-core/src/observability/ring.rs` (new):** `RingEvent {unix_ms,
  level, event, detail}`; a process-global `Mutex<VecDeque>` capped at 128 with
  `record` / `recent` / `clear`; `RingLayer` — a `tracing_subscriber::Layer`
  whose `on_event` visits fields, keeps only events carrying an `event=` field
  (drops `iris.diag` / `iris.kpi` to avoid self-recursion), and pushes a compact
  `message key=value …` line. Adds `tracing-subscriber` to iris-core
  (`Cargo.toml`, Appendix-B scope note). 1 unit test (captures taxonomy events,
  ignores bare debug lines and self-dumps, stays bounded past 128).
- **`crates/iris-android/src/logging.rs`:** `.with(RingLayer)` in the
  subscriber stack.
- **`crates/iris-android/src/engine.rs`:** `FfiLogEvent` record;
  `FfiMeshSnapshot.recent_events` populated from `ring::recent()`.
- **`android/app/src/main/kotlin/iriscore/util/IrisLog.kt` (new):** `IrisLog.d/w`
  → `Log.*("iriscore", "iris.<area> <msg>")`. The 14 adapter call sites in
  `AndroidBleTransportAdapter` / `AndroidWifiDirectTransportAdapter` switched to
  `IrisLog.d("ble.gatt", …)` / `IrisLog.w("wd.init", …)` etc.
- **`MeshViewModel.resolveDiag()`:** appends the last 15 ring events as
  `<level-initial> <event>` : `<detail>` lines.
- Regenerated `iriscode.kt` + `api.kt` alias for `FfiLogEvent`; rebuilt the 3
  `.so`.
- Tests: `cargo test -p iris-core` 772, `-p iris-android` 7;
  `:app:testDebugUnitTest` 78/78.

**Phase T — hardware.** Installed on both phones, launched, drove `/diag`:

Part (a) — one tag:
```
P1  D iriscore: iris.ble.gatt ensureGattServer: addService(IRIS_SERVICE_UUID=3e5c6b1a-…) -> true
P2  D iriscore: iris.ble.gatt …
```
`adb logcat -s iriscore | grep iris.ble` now catches the adapter lines that used
to be under `IrisBleDiag`.

Part (b) — the ring, rendered in the `/diag` console block on **both** phones:
```
> W engine.start_transport_failed   transport failed to start; trying the others
    transport=wifi-aware-0 error=…wifiaware.start: not supported on this device (Other)
> D discovery.scan_pass_complete    discovery pass complete for this transport
    transport=ble-android peers_seen=0
> D discovery.discover_peers_failed discover_peers() failed for this pass; skipping
    this transport this round transport=wifi-aware-0 error=…
```
Time-ordered, newest last — the run-up to a failure reads top-to-bottom. No
crash on either device.

**Phase C.** Commit 78beb32. Tracker **HV-86 → 🟢**.

### Session 05 closeout
- Findings advanced: **HV-86 🟢** (tag unification + event ring, verified on both
  phones). **HV-83 → 🔒 deferred** (satellite post-v1).
- 🟢 count: 4 → 5. **Tier 0: 14 findings — 5 🟢, 7 ✅, 1 🔒 (HV-83), 1 ⬜ (HV-2).**
- **Tier 0 is one finding from done: HV-2.**
- **Gate to Tier 1** = HV-2 only: `cargo test -p iris-android` in CI (HV-1) + the
  L1 sim fault model (HV-4) + `iris_bench` driving two phones with a 10/10
  "P1 sends, P2 receives" + the btsnoop/logcat/dumpsys/meshSnapshot evidence
  pipeline.
- **Next session (HV-2) needs the operator:** (1) enable *Bluetooth HCI snoop
  log* in Developer Options on both phones; (2) be at the bench to run the smoke
  and place the phones. Agent builds the `IrisTestSnippet` APK + the `iris_bench`
  Python package + the evidence pipeline; the smoke uses a tiny payload first to
  isolate the harness from fragmentation (HV-7). **Step 0 before building:**
  check whether the Android engine populates its key directory from the
  discovery handshake's X25519 static ad — the `KeyUnavailable` seen in HV-6's
  manual send suggests it may not, which would block the smoke (new finding
  HV-89).
- Recurring FFI-regen risk (HV-88) still unaddressed — the CI regen-diff gate
  should land as part of HV-2.

---

## Session 06 — 2026-09-01 — HV-2 (the iris_bench harness) + the two blockers it found

**Devices:** P1 = V2205 (`10BCA20F4M000BB`, Android 14 / SDK 34) · P2 = vivo 2004
(`b2fbcd39`, Android 12 / SDK 31). Both on USB, driven by Mobly.
**Build under test:** `56869e4` -> HEAD.
**Session goal:** HV-2 — stand up the Mobly `iris_bench` harness + snippet APK +
evidence pipeline (loop §4). Step 0 first: does Android<->Android delivery even
work today?

---

### HV-2 Step 0 — de-risk: two blockers found

Watched both apps run side by side:
- **BLE discovery works** — `peers_seen=2` on P2.
- **GATT connection failed** every attempt at first (`status=255` / `257`), so
  it looked like "connect never works".
- **HV-89:** the Android engine never calls `set_key_directory` and the
  discovery handshake carries no key ad -> since PRY-33 (`seal_outbound`
  fails closed) *every* addressed Android->Android send dies with
  `KeyUnavailable`. Confirmed: `/send` -> the Android outbox QUEUED chip, core
  counters zero. The old "BLE 3-way working" claim predates PRY-33.

Set `bluetooth_hci_log=1` on both phones; the file isn't readable without root
on vivo, so btsnoop is pulled best-effort via `adb bugreport`.

---

### HV-89 (interim) — give the engine a hand-fed X25519 key directory

**Phase D.** `crates/iris-android/src/engine.rs`: `BenchKeyDirectory` (an
accumulating `Mutex<HashMap<[u8;32],[u8;32]>>` impl `KeyDirectory`), created in
`build()` and wired via `engine.set_key_directory(...)`; FFI
`register_peer_key(peer_hex, x25519_hex)`. The `iris_bench` harness calls
`registerPeerKey` both ways after `startMesh`, using each phone's
`staticX25519()`. Same shape as HV-1's `register_test_key`.
`cargo test -p iris-android` 7/7. Regenerated bindings + `.so`.
**The real fix (handshake-carried `KeyAdvertisementV1` + `TrustKeyDirectory`,
as desktop does) is still owed** — tracked as HV-89.
**Phase C.** Commit c0bfb31.

---

### HV-2 — the iris_bench harness

**Phase R.** Loop §4 (written Session 00) is the research output — Mobly +
Mobly Snippet Lib, host-driven, multi-device. Integration choice: the snippet
is the app module's **`src/androidTest` APK** (`org.iris.mesh.test`) — it runs
in the app's process/classloader so it can `import iriscore.*`, build a real
`IrisEngine` over the real `AndroidBleTransportAdapter` etc., and needs no
separate Gradle module. `mobly-snippet-lib` latest on Maven Central is `1.4.0`
(1.0.0 does not exist).

**Phase D.**
- `android/app/src/androidTest/kotlin/iriscore/snippet/IrisSnippet.kt` — `@Rpc`
  surface: `startMesh` / `stopMesh` / `nodeId` / `staticX25519` /
  `registerPeerKey` / `sendText` / `awaitDelivered` / `meshSnapshot`. Builds the
  engine like `IrisCoreModule` does; installs an `FfiInboxListener` that records
  delivered wire-ids.
- `android/app/src/androidTest/AndroidManifest.xml` — `mobly-snippets` meta-data.
- `android/app/build.gradle.kts` — `testInstrumentationRunner = SnippetRunner`,
  `androidTestImplementation mobly-snippet-lib:1.4.0` + `androidx.test:runner/core`.
- `iris_bench/` — `IrisBenchBase` (2 controllers via `min_number=2`, force-installs
  `app-debug.apk` + `app-debug-androidTest.apk`, `pm grant` the runtime perms,
  battery-opt whitelist, `load_snippet`, per-test evidence, `session-NN` dir),
  `test_tier0_smoke.py` (`P1 -> P2` x10, 2-byte payload = one ATT write),
  `evidence.py` (logcat + `dumpsys bluetooth_manager`/`wifip2p` + `meshSnapshot`
  JSON + best-effort btsnoop via bugreport), `configs/bench_2phone.yml`,
  `__main__.py` (`python -m iris_bench`), `README.md`.
- Config gotchas fixed: `model`/`android` are reserved `AndroidDevice` attrs (use
  only `label`); `suite_runner.run_suite([cls], argv=...)` not `test_runner.main`;
  `--tests` wants `Class.method`.

**Phase T — hardware.** `python -m iris_bench` — full run, `evidence/session-01/`:
```
snippet loaded; node=72ddbbb8…            (P1)
snippet loaded; node=3782eb73…            (P2)
[Test] test_p1_sends_p2_receives_10x
iter 1/10: {'delivered': False, 'timeoutMs': 90000}
iter 2/10: {'delivered': True, ... 'payloadUtf8': 'hi1'}
iter 3..10/10: {'delivered': True, ...}
SMOKE SUMMARY: {"delivered": 9, "of": 10}
```
- The harness drives both phones end to end, loads the snippet, runs the test,
  and captures logcat + both `dumpsys` + `meshSnapshot` JSON on pass and on fail.
- **P1 -> P2 BLE single-hop delivery WORKS** — 9/10, MTU negotiated to 517,
  `gattWrite len=350` succeeds, ~200 ms latency once linked.
- Iter 1 fails: `startMesh` -> first `GATT_Connect` ~33 s later
  (`discovery.connect_ok elapsed_ms=3001` after the scan finally fires), and the
  iter-1 message `msg.delivery_failed "delivery attempt budget exhausted"` /
  `msg.transport_send_failed "transport: not connected"` before the link is up.
  -> **HV-90** (characterised: not "never connects" — "~30 s first connect + the
  queued message is dropped rather than held").

**Phase C.** Commit 0519516. Tracker **HV-2 -> 🟢** (the harness — loop §4.2
deliverable). Regen-diff CI gate + `ANDROID_BUILD_STATUS.md` rewrite still owed.

### Session 06 closeout
- **Tier 0 is complete.** 14 findings — 6 🟢 (HV-2, HV-3, HV-6, HV-84, HV-86,
  HV-88), 7 ✅ (HV-1, HV-4, HV-5, HV-81, HV-82, HV-85, HV-87), 1 🔒 (HV-83
  satellite, deferred).
- New findings: **HV-89** (no Android key exchange — interim landed, real fix
  owed), **HV-90** (first-connect latency + queued-message drop).
- 🟢 count: 5 -> 6.
- **Gate to Tier 1** (§2: the smoke at 10/10) is **9/10** — one HV-90 fix away
  (hold a queued message across link establishment; shrink the first-connect
  window). That is the first Tier-1 work, done with the harness.
- Harness usage: `python -m iris_bench` (see `iris_bench/README.md`). btsnoop is
  best-effort via bugreport on non-root vivo.
- Still owed on HV-2: the CI L1+L2 gate + the bindgen/`.so` regen-diff check
  (HV-88 recurrence), + rewrite `docs/testing/ANDROID_BUILD_STATUS.md`.

---

## Session 07 — 2026-09-01 — HV-90: the tier-0 smoke passes 10/10

**Devices:** P1 = V2205 (`10BCA20F4M000BB`) · P2 = vivo 2004 (`b2fbcd39`).
**Build under test:** `1680610` -> HEAD.
**Session goal:** close the 9->10 gap on the `iris_bench` tier-0 smoke — the §2
Tier-0->Tier-1 gate.

---

### HV-90 (part 2) — hold a message while the link establishes

**Phase R.** `iris_bench/evidence/session-01`: iter 1 of the smoke fails while
iters 2-10 deliver in ~200 ms. Timeline: `sendText` at T0, first `GATT_Connect`
~33 s later, `connect_ok` ~2 s after that. But iter 1's message is already
`DeliveryFailed` — the P2 ring shows
`msg.transport_send_failed error="transport: not connected"` ->
`msg.all_sends_failed "requeuing"` -> `msg.delivery_failed "budget exhausted"`
in quick succession.

Mechanism, traced to source: `deliver_outbound` selects `ble-android` (transport
state `Available` — HV-27: transport state != per-peer link state), calls
`t.send()`, gets `TransportError::NotConnected` (no GATT link yet).
`error.rs::is_retryable()` does **not** list `NotConnected`, so
`any_retryable_failure` stays false ->
`retryable = !saw_failure || any_retryable_failure` = false ->
`requeue_or_fail(item, false)` -> `budget_ok = retryable && attempt <= max` =
false -> `fail_message` **on the first attempt, in milliseconds**. The GATT link
comes up 30 s later with nothing to carry. The `max_attempts` budget (2 for P4)
is designed for ACK retransmission, not for "couldn't send yet" — DTN practice
(RFC 9171: bundles wait in the store until a CLA is available) is to hold.

**Phase D.**
- `crates/iris-core/src/error.rs`: `TransportError::NotConnected` ->
  `is_retryable()` true (permanent unreachability stays `PeerNotFound` /
  `PolicyDenied`).
- `crates/iris-core/src/message_engine/mod.rs`: `deliver_outbound` tracks
  `saw_non_transient_failure` (any failure that isn't `NotConnected`/`Busy`);
  `transient = saw_failure && !saw_non_transient_failure`. New
  `requeue_or_fail(item, retryable, transient)` — a *transient* round holds the
  message (`next_retry = now + 3 s`, no attempt consumed, `PendingSend`) rather
  than the budget path. `no_transport_selected` passes `transient = true` (wait
  for a transport); serialize-failure passes `false`.
- `crates/iris-core/src/transport/simulated.rs`: HV-4 gains
  `not_connected_first_n_sends` (first N `send()`s fail `NotConnected`).
- `crates/iris-core/tests/sysval_fault_injection.rs`:
  `hv90_message_held_until_link_establishes` — two real engines over a shared
  sim transport that rejects the first 3 sends; asserts the P4 message is
  delivered (not failed) and `delivery_failed == 0`. 6/6.
- `cargo test -p iris-core` 772 + all integration suites (only the pre-existing
  HV-83 satellite test fails). `-p iris-android` 7/7.

**Phase T — hardware.** Rebuilt `.so` + both APKs; `python -m iris_bench --tests
test_p1_sends_p2_receives_10x`:
```
iter 1/10:  {'delivered': True, 'latencyMs': 27735, ... 'payloadUtf8': 'hi0'}
iter 2..10: {'delivered': True, 'latencyMs': 250..297}
SMOKE SUMMARY: {"delivered": 10, "of": 10, "median_latency_ms": 281}
[Test] test_p1_sends_p2_receives_10x PASS
```
P2 `meshSnapshot`: `delivered: 10, deliveryFailed: 0` (was `9 / 9` — the reverse
ACK path cleared too). `evidence/session-02`.

**Phase C.** Commit 28d0f1a. Tracker **HV-90 -> 🟢** (meets the §2 gate).
The ~30 s first-connect latency is split out as **HV-91** (iter 1 still takes
27 s — no longer *lost*, just slow).

### Session 07 closeout
- **The Tier-0->Tier-1 gate is met.** `cargo test -p iris-android` in CI (HV-1),
  L1 sim fault model (HV-4), and the `iris_bench` "P1 sends, P2 receives" smoke
  **10/10 with the evidence pipeline** (logcat + dumpsys + meshSnapshot in
  `evidence/session-02`).
- Findings advanced: **HV-90 🟢**. New: **HV-91** (first-connect latency ~30 s).
- 🟢 count: 6 -> 7. **Active tier: 1.**
- **Tier 1 is now open.** Order (loop §2 / §5): HV-91 (first-connect latency) +
  the HV-7/HV-8 group (MTU & fragmentation — a 300-char message both ways 10/10)
  + the HV-11/HV-14/HV-15 scan-lifecycle group. All now testable with the
  harness: add `test_tier1_ble.py`.
- Still owed from Tier 0: HV-89 real fix (handshake key exchange), the HV-2 CI
  gate + regen-diff check + `ANDROID_BUILD_STATUS.md` rewrite.
