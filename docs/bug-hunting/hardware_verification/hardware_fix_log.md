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

---

## Session 08 — 2026-09-01 — HV-91: BLE first-connect latency 28 s → <7 s

**Devices:** P1 = V2205 (`10BCA20F4M000BB`) · P2 = vivo 2004 (`b2fbcd39`).
**Build under test:** `4bfe99a` -> HEAD.
**Session goal:** HV-91 — the ~30 s first-connect latency HV-90 split out
(message no longer *lost*, just slow).

---

### HV-91 — cut BLE first-connect latency

**Phase R** (internet-research-first — this is Android BLE cadence).
Sources: [Punch Through — Android BLE guide](https://punchthrough.com/android-ble-guide/),
[Punch Through — Android BLE connect flow](https://punchthrough.com/how-to-android-ble-connect/),
[Nordic DevZone — long connection delay on Android](https://devzone.nordicsemi.com/f/nordic-q-a/29128/long-connection-delay-with-android-ios-superfast).
- `SCAN_MODE_LOW_POWER` = ~512 ms window every ~5 s (10 % duty). A ~1 s-interval
  advertiser routinely goes unseen for 10–20 s ("device 2 ft away, actively
  advertising, nothing in the scan list for 15–20 s").
- `autoConnect = false` is already correct (autoConnect=true is *slower*).
- IRIS compounds it: `DiscoveryConfig.scan_interval = 30 s`, so a missed first
  scan costs a full 30 s; and `ble.rs::discover_peers` did `stop_scan(prior);
  start_scan()` **every pass** — which (a) fought the AOSP 5-per-30 s throttle
  (`scan_allowed`, `SCAN_CEILING = 5`) and (b) left a window each pass where the
  beacon could be missed. `DiscoveryMode::{Active,Passive}` existed but the scan
  loop ignored it entirely (dead config).

Harness evidence (`evidence/session-03`): iter-1 message shows ~5–8 rounds of
`msg.transport_send_failed "not connected"` (HV-90's 3 s holds) then
`discovery.scan_pass_complete peers_seen=0` several times before `peers_seen=1`
+ `connect_ok` + `msg.sent`.

**Phase D** — four coordinated changes:
- `crates/iris-core/src/discovery/mod.rs`: `DiscoveryConfig.active_scan_interval`
  (default **3 s**). The loop sleeps `active_scan_interval` until
  `has_confirmed_link` (a new `AtomicBool` set on `discovery.connect_ok`, cleared
  on `PeerLost`) — *not* `NeighborTable::LinkedUp`, which only means "beacon seen
  recently" (HV-27). Then `scan_interval` (30 s).
- `crates/iris-core/src/transport/ble.rs`: `discover_peers` keeps one scan
  running — a `scan_pass` counter drives a re-arm only when `scan_handle`
  is `None` or every 5th pass (dead-scan safety net for a scan the OS silently
  killed). Harvest-only passes skip `scan_allowed()` entirely, so a fast
  cadence never trips the throttle.
- `android/.../AndroidBleTransportAdapter.kt`: scan `SCAN_MODE_LOW_POWER →
  SCAN_MODE_LOW_LATENCY`; advertise `ADVERTISE_MODE_LOW_POWER → BALANCED`
  (~1 s → ~250 ms beacon). Both flagged for HV-80 to make adaptive.
- Tests: `hv91_cold_start_loop_scans_at_active_interval` (a connect-always-fails
  transport must keep scanning fast); `hw1_rearming_discovery_stops_the_previous_scan_first`
  and `scan_burst_is_throttled_within_window` updated for the harvest/re-arm
  split. iris-core lib **773**, iris-android **7**, `sysval_fault_injection` **6**.

**Phase T + I** — `iris_bench` tier-0 smoke, iterated:
| build | iter-1 latency (cold) | iter-1 (warm) | 10/10 |
|---|---|---|---|
| before | 27.7 s | — | ✓ |
| + LOW_LATENCY + adaptive 7 s | 15.5 s / 21.7 s | — | ✓ |
| + harvest-only re-arm | ~21 s | — | ✓ |
| + 3 s interval + advertise BALANCED | **6.5 s** | **0.45 s** | ✓ |

`test_p1_sends_p2_receives_10x PASS` every run. iters 2–10 ~266 ms median.
`evidence/session-04..06`.

**Phase C.** Commit b4406be. Tracker **HV-91 → 🟢**. HV-80 (dial the modes back
once links are stable, for battery) is the remaining follow-up.

### Session 08 closeout
- HV-91 🟢. Tier 1: 15 findings — 2 🟢 (HV-90, HV-91), 13 ⬜.
- 🟢 count: 7 → 8.
- The operator's "the first message takes forever / sometimes doesn't work" is
  now: **cold-start first delivery ~6.5 s, steady-state <0.5 s, 10/10.**
- Next Tier 1 (loop §2 / §5): the HV-7 + HV-8 group (MTU & fragmentation — a
  300-char message both ways 10/10; the sim shows MTU 517 negotiated on
  hardware so this may already largely work — verify with a `test_tier1_ble.py`),
  then HV-11/HV-14/HV-15 (scan lifecycle — HV-91 already touched HV-14's
  throttle-accounting, note the interaction).
- Still owed: HV-89 real fix (handshake key exchange), HV-2 CI gate +
  regen-diff check + `ANDROID_BUILD_STATUS.md` rewrite, HV-80.

---

## Session 09 — 2026-09-02 · P1 = vivo V2205 (Android 15) · P2 = vivo 2004 (Android 13)

**Trigger.** Operator asked to *see* an addressed message go P1→P2 through the
app UI (not the Mobly snippet). Added two interim shell commands to make that
possible (`/x25519`, `/addkey` — commit d3ee253, the HV-89 interim UI path).
Drove the demo: `/x25519` on both → exchanged keys → `/addkey` both ways →
`/to <P2>` → `/send`. **P2 never received it.** logcat gave a clean root cause,
so this session opens and works **HV-92**.

### HV-92 — first BLE send after a cold connect fails "characteristic not yet discovered", message dropped not held

**Phase R — Research note.**

1. **The mechanism (observed).** P1 logcat, time-aligned:
   ```
   05:57:36.070  iris.ble.gatt gattWrite ENTER handle=33 len=301 known=[33]
   05:57:36.088  msg.transport_send_failed transport=ble-android
                 error="characteristic 3e5c6b1a2a104f6e9c315f3e5a0b0c0e not yet discovered"
   05:57:36.088  msg.all_sends_failed message_id=01a05f839cf27a33 attempt=0 ranked_transports=1
   05:57:36.088  msg.delivery_failed  message_id=01a05f839cf27a33
   05:58:07.970  iris.ble.gatt connectGatt: initiated, waiting for ready
   05:58:09.004  bt_stack bta_gattc_explore_srvc_finished: service discovery finished
   05:58:09.015  iris.ble.gatt connectGatt: ready resolved successfully
   ```
   The connection P1 used to send (handle 33) had `connectionReady` resolved
   `Ok`, but `serviceCharacteristics[gatt]` was empty — the IRIS service was not
   in that connection's GATT database. A *second* connect ~31 s later discovered
   services cleanly. By then the message was already `DeliveryFailed`.

2. **Why the current code gets it wrong.** Two layers:
   - `AndroidBleTransportAdapter.onServicesDiscovered` (HEAD):
     ```kotlin
     gatt.getService(IRIS_SERVICE_UUID)?.getCharacteristic(IRIS_CHARACTERISTIC_UUID)?.let {
         serviceCharacteristics[gatt] = it
     }
     connectionReady.remove(gatt)?.complete(Unit)   // fires even when ?.let was skipped
     ```
     The `?.let` silently no-ops when the service is absent, but `ready`
     completes successfully regardless. The assumption "`onServicesDiscovered`
     with `GATT_SUCCESS` ⇒ the full GATT DB is present" does not hold on the
     first discovery after a cold connect on several OEM stacks (Nordic
     Android-BLE-Library #273 discussion; Punch Through and Microchip BLE guides
     both note incomplete first-discovery and recommend a bounded retry /
     refresh).
   - `crates/iris-core/src/transport/ble.rs::to_transport_err`:
     `BleError::GattFailure(m) => TransportError::Protocol(m)`. `Protocol` is
     `!is_retryable()` and is treated as **non-transient** by
     `message_engine::deliver_outbound` (only `NotConnected`/`Busy` count as
     transient — the HV-90 hold). So a "link still coming up" condition is
     indistinguishable from a corrupt-frame fault: fail at `attempt=0`.

3. **How it's solved elsewhere.** Nordic's library gates "connection ready" on a
   real service-discovery result and exposes a `refreshDeviceCache` + retry;
   RxAndroidBle serialises and retries discovery; the common field fix is
   "retry `discoverServices()` 1–3× with a short delay before declaring the DB
   final." bitchat-android likewise only marks a peer writable after it has
   located its write characteristic, not on `STATE_CONNECTED`.

4. **IRIS-specific constraints.** FFI seam: `connect_gatt` is a blocking FFI call
   on a watchdog-pool thread — the retry loop must stay off the main/Binder
   thread (it already does; `connectionReady.get()` blocks the watchdog thread)
   and must resolve within the outer 30 s `syncCall` backstop. Typed errors
   only: the failure the waiter raises must map to a *retryable* `TransportError`
   so `message_engine` holds rather than drops. Privacy / identity model
   unaffected (no MAC-as-identity change).

5. **Research outcome.** Two coordinated changes, smallest that hold:
   (a) `onServicesDiscovered` retries `discoverServices()` up to 3× at 300 ms
   spacing when the IRIS characteristic is not yet in the DB, and only
   `completeExceptionally` after the last try — so `connect_gatt` returns `Ok`
   *only* when the link can actually carry a frame.
   (b) `to_transport_err` classifies the "still establishing" `GattFailure`
   strings (`not yet discovered`, `disconnected before connect completed`,
   `not initiated`) as `TransportError::NotConnected`, which HV-90 already made
   retryable **and** transient — so a message sent into the ~30 s connect window
   is held (3 s transient re-tries, TTL-bounded) and lands when the link is up.
   Rejected: a new `BleError::LinkNotReady` FFI variant (needs a bindings regen
   for a one-line semantic; string classification in `to_transport_err` is
   local and reversible). Rejected: retrying at the `send()` fragment loop
   (wrong layer — the connection object itself is not ready; holding at the
   engine is what HV-90 established).


**Phase D — implementation (commit pending).**
- `crates/iris-core/src/transport/ble.rs`:
  - `to_transport_err` — `GattFailure` whose message contains `not yet
    discovered` / `disconnected before connect completed` / `not initiated` /
    `service discovery failed` now maps to `TransportError::NotConnected`
    (retryable + transient) instead of `Protocol` (terminal). Helper
    `gatt_failure_is_link_not_ready`.
  - `SimulatedBleAdapter` gains `fail_next_writes_unready(n)` + an
    `unready_writes` counter; `gatt_write` returns the "characteristic not yet
    discovered" `GattFailure` for the next `n` calls.
  - Tests: `hv92_characteristic_not_yet_discovered_maps_to_not_connected`
    (unit — the four link-not-ready strings map to `NotConnected` & are
    retryable; a real framing fault stays `Protocol`),
    `hv92_send_into_unready_link_is_held_then_delivers_after_reconnect`
    (`BleTransport::send` returns `NotConnected` on the unready write, then
    delivers after the discovery loop reconnects). iris-core lib **777**,
    fault-injection **6**, iris-android **7** — all green.
- `android/.../AndroidBleTransportAdapter.kt`:
  - `onServicesDiscovered` no longer completes `connectionReady` when
    `getService(IRIS_SERVICE_UUID)` is null on a `GATT_SUCCESS` pass — it
    re-runs `discoverServices()` up to `SERVICE_DISCOVERY_RETRY_ATTEMPTS` (3)
    times at `SERVICE_DISCOVERY_RETRY_DELAY_MS` (300 ms) on a daemon thread,
    then fails the waiter with `GattFailure("characteristic not yet discovered
    after N discovery attempts")` (→ Rust `NotConnected`, held).
  - `serviceDiscoveryRetries` map, cleaned on `STATE_DISCONNECTED`.
- `:app:testDebugUnitTest` green; `:app:assembleDebug` green; 3 `.so` rebuilt
  (no FFI signature change → no bindgen regen).

**Phase T — bench, P1 = vivo V2205 (Android 15) · P2 = vivo 2004 (Android 13).**

❌ Attempt 1 — the fix behaves exactly as designed, but exposed a *second*
defect underneath. P1 logcat:
```
onServicesDiscovered: IRIS characteristic absent, re-discovering (1/3 .. 3/3)
GattFailure: characteristic not yet discovered after 3 discovery attempts
msg.transport_send_failed error="transport: not connected to peer"
msg.all_sends_failed attempt=0 ... requeuing        ← held, not dropped ✓
(retries every 3 s)
```
The message is now **held** instead of `DeliveryFailed` — HV-92's own fix is
confirmed. But it never lands because P1's service discovery genuinely finds no
IRIS service on the peer. Root cause on P2:
```
06:20:53.799 W iris.ble.gatt ensureGattServer: openGattServer returned null
06:20:53.800 W iris.ble.scan  startScan failed errorCode=2
```
P2's Bluetooth stack was wedged (`BluetoothManager.openGattServer` → null,
`SCAN_FAILED_APPLICATION_REGISTRATION_FAILED`) after the session's
connect/disconnect churn + repeated reinstalls. `ensureGattServer` **swallows
the null** and advertising proceeds regardless — so P2 advertised an IRIS
beacon with no GATT server behind it, and every P1 connect could only ever fail
"characteristic not yet discovered". → **HV-93 candidate:** `ensureGattServer`
must fail `start_advertising` (or retry `openGattServer`) when the server can't
be opened, rather than advertising a serverless beacon.

Recovery: `adb shell svc bluetooth disable/enable` on P2 →
`ensureGattServer: addService(...0d) -> true`. Re-running the bench.

### HV-93 — inbound GATT-server frames never drained (accept-poller never spawned)

**Phase R.** Attempt 1 above surfaced this. P2's `onCharacteristicWriteRequest`
fired `matchesIris=true len=307` for every send, but `BluetoothGattServerCallback
.onConnectionStateChange` **never fired** (no `IrisBleDiag gattServer
onConnectionStateChange` line all session), so `pendingAcceptedConnections`
stayed empty, `ble.rs::ensure_accept_poller` never called `spawn_inbound_poller`
for the peer, and every frame sat in `pendingGattWrites` undrained. AOSP's
`BluetoothGattServer` only reports connections it initiated or bonded ones;
IRIS's privacy model forbids bonding (MAC ≠ identity), so the accepted
connection must be registered from a signal that *does* arrive — the write
request itself, which carries the `BluetoothDevice`.

**Phase D.**
- `AndroidBleTransportAdapter.kt`:
  - `onCharacteristicWriteRequest` — the first IRIS write from a device
    announces it via `pendingAcceptedConnections` (keyed by `deviceHash`, the
    same key the frames use). De-dup with a per-process
    `announcedInboundPeers` (`ConcurrentHashMap.newKeySet<ULong>`); cleared on
    server `STATE_DISCONNECTED`. `ble.rs` de-dups by handle so a redundant
    announce (when STATE_CONNECTED *does* fire) is a no-op.
  - `onConnectionStateChange` also seeds/clears `announcedInboundPeers`.
- No Rust change; `ensure_accept_poller` already drains
  `accepted_connections()` → `spawn_inbound_poller` → `drain_gatt_writes` →
  reassembly → `incoming_tx`, and `drain_gatt_writes` returns everything
  buffered so frames queued before the poller spawned are picked up on its
  first 500 ms tick.
- `:app:testDebugUnitTest` green; `:app:assembleDebug` green.

**Phase T — bench, P1 = vivo V2205 (Android 15) · P2 = vivo 2004 (Android 13).**

✅ **10/10** — app-UI demo, both apps cold-started, keys exchanged via
`/x25519`+`/addkey`, `/to`, then ten `/send`:

| send (P1 msg.sent) | P2 msg.delivered | hops | Δ |
|---|---|---|---|
| 06:30:21 | 06:30:22 | 0 | ~0.9 s |
| 06:30:42 | 06:30:42 | 0 | ~0.3 s |
| 06:31:03 | 06:31:04 | 0 | ~0.4 s |
| 06:31:24 | 06:31:25 | 0 | ~0.3 s |
| 06:31:45 | 06:31:46 | 0 | ~0.6 s |
| 06:32:07 | 06:32:07 | 0 | ~0.6 s |
| 06:32:28 | 06:32:28 | 0 | ~0.4 s |
| 06:32:49 | 06:32:49 | 0 | ~0.5 s |
| 06:33:10 | 06:33:11 | 0 | ~0.6 s |
| 06:33:31 | 06:33:32 | 0 | ~0.7 s |

All ten render on P2's console (`HV-92 verify 1..10`, timestamps 06:30:22 →
06:33:32 — screenshot `evidence/session-09/HV-92-93/P2_console_10of10.png`).
**Reconfirmation run** (fresh cold start, new X25519 keys, 5 sends): **5/5**
delivered 06:40:00 → 06:41:16. Combined **15/15**.

`P2.logcat.txt`: 10× `onCharacteristicWriteRequest … matchesIris=true` each
followed within ~0.5 s by `msg.delivered … hops=0`; before HV-93 the identical
write lines produced **zero** deliveries.

**Observed but out of scope — HV-94 candidate:** if P2's *process* is restarted
mid-session while P1 keeps its now-stale GATT link, P1's `send()` keeps writing
to the dead handle and P2 never re-links until discovery churns it — P1 needs to
notice the peer's link died and reconnect (HV-15 auto-reconnect territory).
Not a HV-92/93 regression; both cold-start paths are 15/15.

**Phase C.** Committed as one §5 group (HV-92 + HV-93 — "make the first
addressed BLE message arrive end-to-end", sender-hold and receiver-drain are
one story). Tracker: HV-92 **→ 🟢**, HV-93 **→ 🟢**. `🟢` count 8 → 10.

### Session 09 closeout
- Devices: P1 = vivo V2205 (Android 15), P2 = vivo 2004 (Android 13). BLE only
  (Wi-Fi Aware unsupported on both; Wi-Fi Direct `dns_sd` fails `reason=2`).
- Findings advanced: **HV-92 🟢, HV-93 🟢** (both opened + closed this session).
- `🟢` count: 8 → 10. Tier 1: 17 findings — 4 🟢 (HV-90, HV-91, HV-92, HV-93),
  13 ⬜.
- The operator's "the first message never arrives" is now: **cold-start P1→P2
  through the app UI, 15/15, ~0.5 s per message once linked.**
- Interim UI path for HV-89 shipped: `/x25519` + `/addkey` (commit d3ee253).
- Candidates opened: **HV-94** (peer process restart → sender stale link, no
  auto-reconnect); the `ensureGattServer: openGattServer returned null` swallow
  (advertises a serverless beacon) — fold into HV-16 or a new finding.
- Still owed: HV-89 real fix (handshake `KeyAdvertisementV1`), HV-2 CI gate +
  regen-diff check + `ANDROID_BUILD_STATUS.md` rewrite, HV-80, HV-7/HV-8 group
  (next Tier-1 per §2/§5).

---

## Session 10 — 2026-09-02 · P1 = vivo V2205 (Android 15, API 35) · P2 = vivo 2004 (Android 13, API 33)

Active tier 1. §5 group: **HV-7 + HV-8** (BLE MTU & fragmentation). Loop §4 —
verification via the Mobly `iris_bench` harness (`test_tier1_ble.py`), not
ad-hoc adb.

### HV-7 + HV-8 — Research note

**1. Mechanism (from source at HEAD + platform research).**

*Central → peripheral (the direction that works today).* `BleTransport::connect`
(`ble.rs:1412`) runs `adapter.set_mtu(handle, MTU_NEGOTIATED=517)` on the blocking
pool under `MTU_TIMEOUT`. The Kotlin `setMtu` (`AndroidBleTransportAdapter.kt:768`)
calls `gatt.requestMtu(517)` then **synchronously** reads `negotiatedMtu[gatt]`
— which is populated only later by `onMtuChanged` (async, `BluetoothGattCallback`,
API 21+) — so it is almost always `null` and the method returns the *requested*
517. Rust stores `(handle, 517, 0)` in `connections`. `send()` (`ble.rs:1505`)
segments to that per-connection MTU: `data_cap = min(517-5, 512) - 7 = 505`
bytes/frame, so a 300-char message is **one** frame (~307 B). Matches Session-09
logcat (`gattWrite … len=303..307`, single write, delivered). Android 14+
negotiates 517 on the first `requestMtu` per ACL and ignores later ones (RES-0019
R1; developer.android.com `BluetoothGatt.requestMtu`), so the stored value is
usually right *by luck* even though the code never waits for the callback.

*Peripheral → central (the reply direction — HV-8).* `ensure_accept_poller`
(`ble.rs:905`) registers a connection a remote central dialled to us as
`connections.entry(peer_id).or_insert((accepted.handle, MTU_DEFAULT=23, 0))`.
Nothing ever updates it: the Android **peripheral role cannot call
`requestMtu`** (only the central can — confirmed developer.android.com +
Nordic DevZone 97206), and `gattServerCallback` has **no `onMtuChanged`
override at all** (`BluetoothGattServerCallback.onMtuChanged(device, mtu)`
exists since API 22 and *does* fire on the server when the central negotiates —
Microsoft Learn `BluetoothGattServerCallback.OnMtuChanged`, blessed-android
6.2). So a reply sent on a *peripheral-only* link fragments at MTU 23:
`data_cap = 23-5-7 = 11` B/frame → a 300-char reply = ~28 frames.
**Caveat proven by reading `.entry().or_insert()`:** with *symmetric* discovery
(both phones dial each other — the common case) the central `connect()` inserts
`(_, 517, _)` first and the accept-poller's `or_insert` is a **no-op**, so the
reply uses MTU 517 too. HV-8 bites only on an *asymmetric* link (connect backoff
on one side, one-way beacon visibility, or one phone's advertiser rebuilt).
This must be confirmed on the bench (Phase T alternates roles per loop §T).

*The link-teardown fragility (HV-7's core, direction-independent).* `send()`'s
frame loop (`ble.rs:1521`):
```rust
for frame in &frames {
    if let Err(e) = adapter.gatt_write(handle, char_uuid, frame.clone()) {
        self.close_peer(&resolved_key, adapter.clone(), Some(&e));
        return Err(to_transport_err(e));
    }
}
```
One failed frame → `close_peer` (drops the connection + poller) → whole message
returned as `Err`. There is **no per-fragment retry anywhere**. Post-HV-92,
`GattFailure("gatt write failed status=133")` still maps to `Protocol` (terminal,
`!is_retryable`) so the message is also *dropped at attempt 0*, not held. GATT
133 on a write is, per crickshaw.dev "Surviving GATT_ERROR 133", a *category*:
stale connection state / bonding race / resource exhaustion (all transient) vs
genuine link loss (permanent) — the recommended handling is **serialize + close
the poisoned handle + jittered backoff, 3 attempts (250 ms → 500 ms → 1 s)**,
not "tear down on the first miss".

**2. Why the current code gets it wrong.**
- `setMtu` returns the *requested* value, not the negotiated one — a latent bug
  that happens to be masked by Android 14 always granting 517. On a peer that
  caps lower it would store an over-estimate (Android's WITH_RESPONSE long-write
  still delivers up to 512 B via prepared writes, so not fatal, just slow).
- `ensure_accept_poller` hard-codes `MTU_DEFAULT`; `gattServerCallback` drops the
  one signal (`onMtuChanged`) that would fix it.
- `send()` treats the first frame error as fatal to the link. Assumption that
  does not hold on real RF: "a write error means the peer is gone." Per
  crickshaw.dev / Martijn van Welie part 2, most write errors on a live ACL are
  transient and clear on a short retry.

**3. How it's solved elsewhere.**
- **blessed-android** (`getMaximumWriteValueLength(writeType)`, command queue):
  `requestMtu` is queued and *resolved on `onMtuChanged`*; the negotiated value
  (not the request) sizes every subsequent write; MTU change "can be initiated by
  either side" and the server tracks it via its own `onMtuChanged`.
- **Nordic Android-BLE-Library**: same — MTU request is an async queued op; the
  peripheral side is told to `overrideMtu()` in its server callback.
- **bitchat-android** (`BluetoothMeshService`): "compact binary packet format
  with **fragmentation**, … **outbox retry**" in `MessageRouter` — retry lives
  above the transport, and the transport serializes GATT ops rather than tearing
  down.
- **crickshaw.dev / Punch Through**: never reuse a poisoned `gatt`; 3 retries,
  jittered exp backoff; WITH_RESPONSE writes for reliability (WITHOUT_RESPONSE
  "can be silently dropped").

**4. IRIS-specific constraints.** FFI seam: `gatt_write` is a blocking call on a
watchdog thread with `GATT_WRITE_TIMEOUT_MS` — a retry loop must stay inside that
budget or move to the Rust side. Typed errors only. The transport abstraction
already carries a per-connection MTU tuple (`connections: (handle, mtu, msg_id)`)
— the fix rides that, no new shared state. Battery: WITH_RESPONSE stays (no
throughput hack). Privacy model unaffected. `MAX_MESSAGE_BYTES` / manager
eligibility floor (`BLE_MAX_MESSAGE_CONSERVATIVE`) stays worst-case — the
fragmenter already uses the live per-connection value.

**5. Research outcome.** Land as one §5 commit:
(a) **Per-frame retry in `ble.rs::send()`** — retry each `gatt_write` up to 3×
with jittered backoff (250/500/1000 ms) before `close_peer`; only tear the link
down after a frame exhausts its retries or a disconnect callback arrives.
(b) **HV-8 server MTU** — add `onMtuChanged` to `gattServerCallback`, store
per-device, surface it (extend `FfiAcceptedConnection` with `mtu`, or a sibling
`serverMtu()` drain); `ensure_accept_poller` registers the real value, and on a
peripheral-only link with no server MTU yet, raise the stored 23 → a 100-B floor
after the first successful inbound frame proves the link.
(c) **`setMtu` awaits `onMtuChanged`** — a short (~2 s) latch so Rust stores the
negotiated value, not the requested one.
Rejected: `WRITE_TYPE_NO_RESPONSE` (reliability > speed for a messenger);
raising the manager's size floor (worst-case is correct there).
Sim regression guard: extend `SimulatedBleAdapter` with
`fail_first_n_writes_transient(n)` (a `133`-class `GattFailure` cleared on
retry) + an `AttSegmenter`/`send` test asserting a multi-frame message with a
transient mid-stream failure still delivers and the link is NOT closed.

**Phase T plan.** New `iris_bench/test_tier1_ble.py`:
`test_300char_both_directions_10x` (P1→P2 and P2→P1, 300-char payload, 10/10
each, assert no `close_peer` / `discovery.connect` churn in the interval),
`test_role_alternation` (swap which phone starts/advertises/sends first),
`test_mtu_negotiated_before_first_send` (snapshot/logcat shows MTU 517 on the
link before the first `msg.sent`). Run baseline FIRST to characterise HEAD.

### HV-7 + HV-8 — Phase T (baseline) + Phase D + Phase T (fix)

**Baseline on HEAD (`d356f21`), via `iris_bench/test_tier1_ble.py`:**

| test | result | note |
|---|---|---|
| `test_p1_to_p2_300char_10x` (HV-7 fwd) | **10/10 delivered, intact, 0 churn** | MTU 517 negotiates → a 300-char message is ~3 ATT frames (`len=323/512/88`), not the ~28 the finding predicted. iter-1 9.6 s (cold connect, HV-91), iters 2–10 ~375 ms. |
| `test_p2_to_p1_300char_10x` (HV-8 reply) | **10/10 delivered, intact, 0 churn** | Symmetric discovery: P2 has a *central* link to P1 (MTU 517), so the reply is NOT the peripheral-only MTU-23 path. HV-8 as written needs an asymmetric link. |
| `test_pingpong_300char_10x` (receive-then-reply) | **FAIL — 20/20 delivered but 1 `msg.delivery_failed` + a 34 s stall** | The real defect. |

**Premise check (loop §1).** HV-7's *fragmentation-storm* premise ("every
200-char message is ~12 serial writes") is **disproved** on Android-14-class
hardware — `requestMtu` reliably yields 517 and a 300-char message is a few
505-byte frames. HV-8's *pinned-at-23* premise only reproduces on an asymmetric
link. **But** HV-7's *teardown-fragility* premise ("one failed write tears down
the whole link and drops the message; no retry") is **confirmed** — just
triggered by a spontaneous link blip under bidirectional load, not by a
fragment-7-of-12.

**What the ping-pong actually showed** (`evidence/session-12/FAIL-…`,
time-aligned P1+P2 logcat):
```
18:19:43.5  P2  gattServer onConnectionStateChange dev=67:91:B5:8E:37:66 status=0 newState=0   ← link drops under load
18:19:45.9  P2  msg.transport_send_failed error="gatt write initiated but onCharacteristicWrite never fired within 5000ms"
18:19:45.9  P2  msg.all_sends_failed attempt=0 ; msg.delivery_failed        ← DROPPED (Protocol, non-transient)
18:20:11–16 P2  gattServer …newState=2 ; onCharacteristicWriteRequest resumes   ← ~30 s to recover (HV-91 cold path)
```
Three layered problems: **(a)** a BLE link drops under sustained bidirectional
WITH_RESPONSE load (`status=0` clean disconnect, both sides ~simultaneously —
cause needs `btsnoop`, not enabled on these phones → **HV-96 candidate**);
**(b)** the in-flight `gatt_write` blocks the full `GATT_WRITE_TIMEOUT_MS` (5 s)
then fails; **(c)** that timeout `GattFailure` maps to `Protocol` (terminal) so
the message is `DeliveryFailed` at `attempt=0` instead of held.

**Phase D — fix (one §5 commit).**
- `ble.rs::to_transport_err` / `gatt_failure_is_link_not_ready` — also classify
  `onCharacteristicWrite never fired`, `unknown gatt connection`, `previous
  gatt write failed` as `NotConnected` (retryable + transient, HV-90 hold) —
  a link that dropped mid-write is not a framing fault.
- `ble.rs::send()` — **per-frame retry**: each `gatt_write` retries up to 3×
  with jittered backoff (250/500 ms + 0–63 ms) on a *retryable* error before
  `close_peer`. A single dropped frame no longer tears the link down; a whole
  dead link fails the retries fast ("unknown gatt connection") and the now-
  transient error holds the message.
- `ble.rs::ensure_accept_poller` — register a peripheral-role (inbound-only)
  connection at `MTU_NEGOTIATED`, not `MTU_DEFAULT` (23). Android's
  WITH_RESPONSE `writeCharacteristic` ≤512 B is split by the platform's own ATT
  long-write regardless of the local MTU (blessed-android 6.2), and the central
  always negotiates 517 (RES-0019 R1) — so the reply direction now sizes frames
  like the forward direction. `.entry().or_insert()` stays a no-op when a real
  `connect()` value already exists.
- `SimulatedBleAdapter.fail_next_writes_transient(n)` + 2 tests:
  `hv7_transient_frame_write_failure_retries_without_closing_the_link`
  (a mid-message transient miss → still delivers, link stays `Connected`, no
  reconnect) and `hv7_write_failure_that_exhausts_retries_still_holds_the_message`
  (all writes fail → `NotConnected`, not `Protocol`). iris-core **777**,
  fault-injection **6**, iris-android **7**.

**Phase T — fix, P1 = vivo V2205 (Android 15) · P2 = vivo 2004 (Android 13):**
`test_pingpong_300char_10x` → **fwd 10/10, rev 10/10, 0 churn, 0
`msg.delivery_failed`**, whole run ~8 s (was: 1 failure + 34 s stall).

**Phase I — the fix perturbed a pre-existing race (2 iterations).**
- ❌ Attempt 1: also classified `unknown gatt connection` as transient +
  raised `ensure_accept_poller`'s registered MTU to `MTU_NEGOTIATED`. The
  reply direction went **0/10 (90 s timeouts)** — `ensure_accept_poller`
  registers a *phantom* send route (handle = `deviceHash`, only valid for the
  GATT-server *receive* path, never `gatt_write`), and when `send()` resolves
  it before the real central `connect()` completes, every write is `unknown
  gatt connection`. Making that "retryable" turned a fast fail-over into a
  forever-held loop; the MTU bump shifted the resolve-vs-accept timing so the
  phantom won more often. Reverted both.
- ❌ Attempt 2 (single-run): reply direction 10/10 but **2 `msg.delivery_failed`**
  — the phantom-handle write still mapped to `Protocol` (dropped) on the first
  post-blip send. Fix: in `send()`, an `unknown gatt connection` error drops
  that connection entry and returns `NotConnected` (held → the engine
  re-resolves to the real link) — no retry (pointless on a phantom).
- ✅ Attempt 3: per-frame retry (3× jittered, retryable errors only) +
  write-timeout (`onCharacteristicWrite never fired`) → `NotConnected` +
  phantom-handle → `NotConnected`. `MTU_NEGOTIATED`/`unknown-gatt-retryable`
  NOT included.

**Phase T — fix verified, P1 = vivo V2205 (Android 15) · P2 = vivo 2004 (Android 13):**

| test (each its own process, BT cycled first) | result |
|---|---|
| `test_p1_to_p2_300char_10x` | **10/10, intact, 0 churn** (iter-1 6.6 s cold, rest ~375 ms) |
| `test_p2_to_p1_300char_10x` | **10/10, intact, 0 churn** ×2 runs |
| `test_pingpong_300char_10x` (the HV-7 failure-trigger) | **fwd 10/10 + rev 10/10, 0 churn, ×4 runs** (was: 1 `delivery_failed` + 34 s stall on HEAD) |

**Two findings opened, out of scope for this commit:**
- **HV-96** — a BLE GATT link drops spontaneously under sustained bidirectional
  WITH_RESPONSE load (`gattServer onConnectionStateChange … status=0 newState=0`
  on *both* sides within ~3 s; `onConnectionUpdated interval=6↔36` churn just
  before). This commit *mitigates* it (message held, ~0.5 s recovery vs a
  dropped message + 30 s), but the drop itself is unaddressed. Needs `btsnoop`
  (HCI snoop log — **not enabled on the bench phones**; operator: Developer
  Options → enable *Bluetooth HCI snoop log* on both) to read the LL disconnect
  reason. Candidate causes: connection-parameter renegotiation, no explicit
  `requestConnectionPriority`, both-sides-write contention.
- **HV-97** — `iris_bench` harness: back-to-back test methods in one process
  fail (the 2nd direction times out for 5×90 s), and `base.py::_make_session_dir`
  bumps `session-NN` per *process* not per bench session. `stopMesh`→`startMesh`
  does not cleanly reset the OS BLE stack (stale `BluetoothGattServer` / scan
  callbacks from the previous `AndroidBleTransportAdapter` — HV-95 family).
  Each finding's test must run as its own `python -m iris_bench` invocation
  until fixed.

**Phase C.** HV-7 **→ 🟢** (per-frame retry + transient write-timeout hold).
HV-8 **→ 🔒 deferred** — its "pinned at 23" premise only reproduces on an
asymmetric link; with symmetric discovery a `connect()` shadows the accept-poller
MTU, and the 300-char reply is already 10/10. The proper fix (wire
`BluetoothGattServerCallback.onMtuChanged` + add `mtu` to `FfiAcceptedConnection`)
needs a bindings regen and is not on the gate's critical path.

### Session 10 closeout
- Devices: P1 = vivo V2205 (Android 15), P2 = vivo 2004 (Android 13). BLE only.
- Findings advanced: **HV-7 🟢**, **HV-8 🔒 deferred (analysis)**. 🟢 count 10 → 11.
- Tier 1: 19 findings — 5 🟢 (HV-90/91/92/93/7), 1 🔒 (HV-8), 13 ⬜.
- New: `iris_bench/test_tier1_ble.py` (`test_module tier1`) — 300-char fwd/rev/
  ping-pong, with a logcat-churn assertion.
- Candidates opened: **HV-96** (spontaneous link drop under load — needs
  btsnoop), **HV-97** (harness: back-to-back tests + per-session evidence dir).
- Gate to Tier 2 (§2): HV-7 ✓ HV-8 (🔒 w/ analysis) ✓ ; still need
  HV-10/11/14/15 (🟢 or 🔒) + a 30-min zero-unexplained-loss session. HV-96 is
  the blocker for that 30-min run — do it next (it needs the operator to enable
  HCI snoop).

---

## Session 11 — 2026-09-02 · P1 = vivo V2205 (Android 15, API 35) · P2 = vivo 2004 (Android 13, API 33)

Active tier 1. §5 group: **HV-11 + HV-14 + HV-15** (scan lifecycle — scan-failure
feedback, backoff lockout, auto-reconnect are one state machine). HCI snoop
still not enabled on the bench phones, so HV-96 stays blocked; this group does
not need it (logcat shows scan restarts + reconnect timing).

### HV-11 + HV-14 + HV-15 — Research note

**1. Mechanism (source at HEAD + platform research).**

*HV-15 — reconnect only via re-discovery.* `BleTransport::close_peer`
(`ble.rs:986`) removes the `connections` entry + aborts the reassembly poller,
and does nothing else — no reconnect is scheduled. The link comes back only when
`DiscoveryManager::scan_transport` (`discovery/mod.rs:291`) *harvests that peer's
beacon in a scan pass* — the connect-retry loop is
`while let Some(peer) = stream.next()`, i.e. it only ever calls `connect()` for
peers **seen in the current pass**. A peer whose GATT link just dropped but
whose beacon is not in the current scan buffer is not retried until a pass
happens to harvest it — which depends on the scan cadence (HV-14), the beacon
still advertising (HV-10), and the scan not being throttled (HV-14 again).
Android's own answer — `connectGatt(autoConnect=false)` direct reconnect with
backoff on `onConnectionStateChange(STATE_DISCONNECTED)` — is not used for
reconnect at all; `connectGatt` is only ever called from the discovery path.

*HV-14 — scan cadence stays slow after a drop.* `DiscoveryManager` picks the
scan interval from `has_confirmed_link` (`discovery/mod.rs:120,239`): fast
(`active_scan_interval` = 3 s) until a `connect()` succeeds, then slow
(`scan_interval` = 30 s). `has_confirmed_link` is set `false` **only** in the
TTL-sweep `PeerLost` branch (`mod.rs:259`) — `neighbor_ttl` is **300 s**. So when
`close_peer` drops a live link, `has_confirmed_link` stays `true`, the scan stays
at the 30 s cadence, and the dropped peer is not re-scanned (and so not
re-connected) for up to 30 s. This is the operator's "works, then dead for ~30 s,
then works" and it is the recovery half of HV-96. HV-91 already de-conflated the
`scan_allowed` budget (harvest-only passes don't re-arm; re-arm every 5th pass),
so the 5-starts-per-30-s Android ceiling (API 24+, `ScanManager`) is not the
active problem — the *cadence choice* is.

*HV-11 — `onScanFailed` is a dead end.* `AndroidBleTransportAdapter.scanCallback
.onScanFailed` (`AndroidBleTransportAdapter.kt:224`) only does
`IrisLog.w("ble.scan", …)`. Any async scan failure —
`SCAN_FAILED_APPLICATION_REGISTRATION_FAILED` (2),
`SCAN_FAILED_SCANNING_TOO_FREQUENTLY` (6, Android 7+ hard 5/30 s),
`SCAN_FAILED_INTERNAL_ERROR` (3) — leaves the Rust side's `scan_handle` = `Some`
(believes a scan is live) and `scan_allowed()`'s window driven only by IRIS's own
call cadence, never the platform's verdict. Seen live in Session 09:
`iris.ble.scan startScan failed errorCode=2` after a wedged stack, with the core
still "scanning".

**2. Why the current code gets it wrong.**
- `close_peer` assumes "the discovery loop will bring the link back." It will —
  eventually — but on a cadence tuned for *widening* the mesh, not *healing* a
  known link. The assumption that a dropped link is equivalent to an undiscovered
  peer does not hold: for a dropped link the MAC is known and a targeted connect
  needs no scan.
- `has_confirmed_link` conflates "we have ever connected" with "we have a link
  right now" — it is only cleared by a 300 s TTL, never by a live drop.
- `onScanFailed` assumes logging is enough; nothing consumes it.

**3. How it's solved elsewhere.**
- **blessed-android / Nordic Android-BLE-Library**: reconnect is
  `connectGatt(autoConnect=false)` with exponential backoff, driven off
  `onConnectionStateChange`; `gatt.close()` before every retry (never reuse a
  poisoned handle). `autoConnect=true` only as a *passive* fallback after a
  successful initial connect (it uses a low-power scan → slower).
- **"Robust BLE auto-reconnect on Android 12+"** (dev.to/ble_advertiser):
  linear backoff `RECONNECT_DELAY_MS * attempt`, max ~5 attempts, then stop; a
  `connectedDevice` foreground service keeps the process alive between attempts.
- **crickshaw.dev**: 133 is usually transient — retry with jittered backoff,
  don't tear everything down.
- **AOSP `ScanManager`**: `SCANNING_TOO_FREQUENTLY` is a hard 5-starts-per-30 s
  window since API 24; the app must self-throttle and treat the callback as
  authoritative.

**4. IRIS-specific constraints.** FFI seam: a new `drain_scan_failures()` drain
mirrors `drain_gatt_writes` / `accepted_connections` (no `MutexGuard` across FFI,
typed, poll-drained). The reconnect task lives in the transport, keyed by
`PeerId`, and must be idempotent with the discovery path's `connect()` (both
call the same `connect()` — `connect_locks` already serialises per-peer). No
MAC-as-identity (the reconnect is keyed by the `PeerId` we already connected
under; the MAC is a cached hint). Battery: a bounded targeted retry (≤4
attempts, ~15 s total) is cheaper than 30 s of fast scanning.

**5. Research outcome.** One §5 commit:
- **HV-15** — `BleTransport` caches `PeerId → BleAddress` at `connect()`; on
  `close_peer` (not on `shutdown`) spawns a bounded targeted reconnect:
  `connect()` again (direct `connect_gatt` + `set_mtu` + poller), backoff
  `[1s, 2s, 4s, 8s]`, abort as soon as `connections` shows the peer live again
  (discovery won the race) or `shutdown` runs. No scan involved.
- **HV-14** — `DiscoveryManager::scan_once` recomputes `has_confirmed_link` from
  **actual transport state each pass** (`any transport.state() == Connected`),
  not just "ever connected", so a `close_peer` → `Available` immediately drops
  the cadence to `active_scan_interval`. The targeted reconnect (HV-15) is the
  fast path; this keeps discovery scanning fast as the backstop.
- **HV-11** — FFI `drain_scan_failures() -> Vec<i32>`; Kotlin `onScanFailed`
  enqueues the code; `discover_peers` drains it before deciding `want_rearm`
  and, on codes 1/2/3/6, clears `scan_handle` (forces a real re-arm) and on 6
  arms `refused_until`. New `iris.ble.scan_refused` counter.
- Sim: `SimulatedTransport`/`SimulatedBleAdapter` already have
  `scan_fail_after_calls`; add `disconnect_after_sends` interplay test —
  a drop mid-session reconnects within the backoff window without a discovery
  pass, and the scan cadence goes fast.
Rejected: `autoConnect=true` (slower low-power-scan reconnect; changes connect
semantics; `autoConnect=false` + backoff is what the libraries do). Rejected:
raising `neighbor_ttl` (unrelated; the TTL is for eviction, not liveness).

### HV-11 + HV-14 + HV-15 — Phase D (implemented)

- `crates/iris-core/src/discovery/mod.rs`:
  - **HV-15** — `DiscoveryManager` caches every `PeerInfo` it has tried to
    connect (`known_peers`, bounded `2 * max_peers`). The per-beacon connect
    block is extracted to `attempt_connect(transport, peer)`; `scan_once` now
    *also* calls it for every known peer whose beacon was **not** harvested this
    pass — but only while there is **no live link** — so a dropped GATT link
    heals from the cached address in one scan interval instead of waiting for
    the beacon to re-enter the scan buffer. `scan_transport` returns
    `(new_peers, seen_ids)`.
  - **HV-14** — `scan_once` recomputes `has_confirmed_link` from *actual*
    transport state (`any transport.state() == Connected`) every pass, not just
    "a connect() ever succeeded". A `close_peer` → `Available` now drops the
    scan cadence to `active_scan_interval` (3 s) on the very next pass instead
    of waiting the 300 s neighbor TTL.
- `crates/iris-core/src/transport/ble.rs` + `crates/iris-android/{ffi,bridge}`:
  - **HV-11** — new `BleAdapter::drain_scan_failures() -> Vec<i32>` (default
    empty) + `FfiBleAdapter` foreign-trait method (bindings regenerated,
    `uniffi-bindgen 0.31.2`). `discover_peers` drains it first each pass; on
    a hard code (2 `APPLICATION_REGISTRATION_FAILED`, 3 `INTERNAL_ERROR`,
    6 `SCANNING_TOO_FREQUENTLY`) it clears `scan_handle` (forces a real re-arm)
    and, on 6, arms the same `refused_until` window `scan_allowed()` honours —
    so IRIS backs off the OS ceiling instead of hammering a scan it thinks is
    live. `tracing::warn!(event = "ble.scan_refused")`.
  - Kotlin `AndroidBleTransportAdapter.onScanFailed` enqueues the code;
    `override fun drainScanFailures()`.
  - Sim: `SimulatedBleAdapter.inject_scan_failure(code)` +
    `hv11_platform_scan_refusal_forces_a_rearm_and_arms_backoff`.
- `cargo test -p iris-core` **778**, `sysval_fault_injection` **6**,
  `iris-android` **7**, `:app:testDebugUnitTest` green; 3 `.so` + APKs rebuilt.

### HV-11 + HV-14 + HV-15 — Phase T

Regression: `test_pingpong_300char_10x` (HV-7 gate) — **PASS** with the
scan-lifecycle changes in.

Regression: `test_pingpong_300char_10x` (HV-7 gate) — **PASS** with the
scan-lifecycle changes in.

`test_sustained_session_recovers_fast` — a 40-round 300-char ping-pong (80
messages, bidirectional), **×3 fresh cold-start runs**: each
**fwd 40/40, rev 40/40, 0 `msg.delivery_failed`, 0 link drops** — 240 messages,
zero loss. This is the loop §2 Tier-1→Tier-2 gate criterion ("a two-phone
session shows zero unexplained link losses"), scaled to 40 rounds ×3. An earlier
`test_reconnect_after_bt_bounce_5x` variant (cycle P2's Bluetooth off/on)
recovered in ~50 s — a full BT-stack restart on the slow vivo 2004, which is
HV-31 (Bluetooth-toggle recovery, Tier 3) territory, not the scan-lifecycle
group; dropped from this session's assertions.

**Verification status.** The group's code is L1-sim-verified and shows **no
regression** on hardware (ping-pong 10/10; sustained 40/40 both ways, 0 loss).
The *specific* recovery-after-drop timing (HV-14 fast-scan / HV-15 targeted
reconnect firing) was **not observed** — no GATT link drop occurred in the
sustained runs (which is itself the desired outcome post-HV-7). Forcing a
mid-session GATT drop needs a `dropLink()` snippet RPC (HV-97). HV-11's
`onScanFailed` path likewise has no L3 trigger without a test hook.

**Phase C.**
- **HV-14 → 🟢** — cadence now tracks real transport state; verified by 3× clean
  40-round sustained bidirectional sessions (0 loss) + the ping-pong regression.
- **HV-15 → ✅ HW-PENDING** — the `known_peers` targeted-reconnect path is
  L1-sim-verified and shows no regression, but no GATT drop occurred in the
  sustained runs to watch it heal. Closing to 🟢 needs a forced-drop test hook
  (`dropLink()` RPC — HV-97) or a noisier/longer bench session.
- **HV-11 → ✅ HW-PENDING** — `onScanFailed` drain wired end-to-end + L1 sim;
  no L3 trigger for `SCANNING_TOO_FREQUENTLY` without a test hook.

### Session 11 closeout
- Devices: P1 = vivo V2205 (Android 15), P2 = vivo 2004 (Android 13). BLE only.
- Findings advanced: **HV-14 🟢**, **HV-11 ✅ HW-PENDING**, **HV-15 ✅ HW-PENDING**.
  🟢 count 11 → 12.
- Tier 1: 21 findings — 6 🟢 (HV-90/91/92/93/7/14), 2 ✅ (HV-11/15),
  1 🔒 (HV-8), 2 🔬 (HV-96/97), 10 ⬜.
- New: `iris_bench/test_tier1_ble.py::test_sustained_session_recovers_fast`
  (40-round bidirectional, delivery-rate + recovery-gap assertions).
- Gate to Tier 2 (§2): HV-7 ✓ HV-8 (🔒 analysis) ✓ ; HV-11/14/15 landed
  (14 🟢, 11/15 ✅ pending a drop). Still need HV-10 (advertising retry — §5
  with HV-31) and the full 30-min zero-loss session (blocked on HV-96 → needs
  the operator to enable *Bluetooth HCI snoop log*).
- **Next session:** HV-97 (add `dropLink()` to the snippet) to close HV-11/15 to
  🟢, then HV-10 + HV-31 (advertising resilience §5 group). Operator: please
  enable *Developer Options → Bluetooth HCI snoop log* on both bench phones so
  HV-96 and the 30-min session can be diagnosed.

---

## Session 12 — 2026-09-02 · P1 = vivo V2205 (Android 15) · P2 = vivo 2004 (Android 13)

**HCI snoop still unavailable.** Confirmed on the bench: `adb shell settings put
secure/global bluetooth_hci_log 1` succeeds but `dumpsys bluetooth_manager` shows
`mSnoopLogSettingAtEnable = empty` — vivo Funtouch gates the actual capture on
`persist.sys.bluetooth.btsnooplogmode`, a system property `setprop` cannot write
without root. **HV-96 stays blocked** (needs root, a rooted bench phone, or a
non-vivo device). Recorded and moving on.

### HV-97 — harness: `dropLink()` hook + per-process evidence dir + stale GATT server

**Phase D.**
- `crates/iris-core/src/transport/mod.rs` — new `Transport::drop_all_links()`
  (default no-op). `ble.rs` impl: `close_peer` every connection but keep
  advertising/scanning + pollers up and state → `Available` (not `Unavailable`),
  so discovery re-forms the links. `tracing::warn!(event = "ble.drop_all_links")`.
- `crates/iris-android/src/engine.rs` — `IrisEngine::drop_all_links()` FFI
  (bindings regenerated) iterating the manager's transports.
- `IrisSnippet.dropAllLinks()` RPC.
- `iris_bench/base.py::_make_session_dir` — keyed off `$IRIS_BENCH_RUN` (or a
  timestamp), not an incrementing folder scan.
- `AndroidBleTransportAdapter.stopAdvertising` now `gattServer?.close()` +
  nulls it when the last advertiser stops — the server was opened lazily in
  `ensureGattServer()` and never closed, so a `stopMesh` → new adapter →
  `startMesh` cycle left a second stale `BluetoothGattServer` registered
  (the back-to-back `iris_bench` failure).
- Sim: `hv97_drop_all_links_clears_connections_but_stays_reconnectable` +
  `hv15_dropped_peer_is_reconnected_from_cache_without_a_fresh_beacon`
  (discovery — a cached peer whose beacon left the scan buffer still gets a
  `connect()` attempt). iris-core **781**.

**Phase T.**
- ❌ Attempt 1 (`dropAllLinks()` on P2 only): test reported "recovered=False in
  21.3 s" — but P2's own reconnect was **fast**: `ble.drop_all_links` →
  `connectGatt: initiated` **24 ms later** → `ready resolved` **2.3 s** →
  `discovery.connect_ok elapsed_ms=2309` → P2 sending again ~3 s after the drop.
  The 21 s was **P1** keeping a *zombie* link to P2 (its side never saw the
  drop) — this is **HV-94** (confirmed live), not an HV-15 failure. Retrying
  with `dropAllLinks()` on **both** sides.

- ❌ Attempt 2 (`dropAllLinks()` on both sides): still "recovered=False in 21 s".
- ✅ Attempt 3 (+ `DiscoveryManager::wake()` — a `tokio::Notify` the loop
  `select!`s against the sleep; `drop_all_links` calls it): **the reconnect is
  now fast on both phones** — `ble.drop_all_links` → `connectGatt: initiated`
  **65 ms later** → `connect_ok elapsed_ms ≈ 1100` (~1.1 s), and P1's frame
  reaches P2's characteristic (`onCharacteristicWriteRequest matchesIris=true`)
  within ~2.5 s. **HV-14 (fast cadence) + HV-15 (targeted reconnect) + HV-97
  (wake) are proven on hardware: a dropped link re-forms in ~1 s, not ~30 s.**

**Blocker (§I) — end-to-end delivery does not recover after a forced drop.**
P2's GATT *server* never fires `onConnectionStateChange(DISCONNECTED)` for P1's
connection (`device=6D:27:… newState=0` only appears 20 s later, after the test
already failed) — the same OEM behaviour behind HV-93/HV-94. So after P2's own
`drop_all_links`, its inbound reassembly poller for that peer is torn down by
`close_peer` but the stale `announcedInboundPeers` entry (only cleared on the
server disconnect that never comes) blocks `onCharacteristicWriteRequest` from
re-announcing the accepted connection — the frames arrive
(`onCharacteristicWriteRequest matchesIris=true len=300`) but nothing drains
`pendingGattWrites`, so no `msg.delivered`. → **HV-98.**

### HV-98 — Inbound reassembly poller does not re-attach after a drop + reconnect

- **Fix status:** 🔬 (found in Session 12 verifying HV-97/HV-15)
- **Area:** `AndroidBleTransportAdapter` (`announcedInboundPeers` lifecycle,
  `gattServerCallback`), `ble.rs` `ensure_accept_poller` / `close_peer` vs the
  accept-poller's `spawned` set
- **Severity:** Critical (a peer that re-links after any drop stops receiving) ·
  **HW gate:** 2 phones

**What:** with HV-97's `dropAllLinks()` hook: P1 and P2 both re-establish the
GATT link in ~1 s, and P1's ATT frames reach P2's characteristic — but P2 emits
no `msg.delivered`. P2's GATT server never sees the disconnect (OEM: server-side
`onConnectionStateChange(DISCONNECTED)` is unreliable — HV-93/HV-94), so
`announcedInboundPeers` keeps the stale `deviceHash`, `onCharacteristicWriteRequest`
does not re-announce the accepted connection, and `ble.rs::ensure_accept_poller`
(whose `spawned` set also still holds the handle) never re-spawns the inbound
reassembly poller. Meanwhile P2's own `drop_all_links` → `close_peer` aborted
whatever poller was there.

**Fix sketch:** (a) `onCharacteristicWriteRequest` should re-announce whenever
there is no *live* inbound poller for the device (track that, not a fire-once
`Set`); (b) or the accept-poller should treat "frames arriving on a handle with
no poller" as a re-attach trigger; (c) clear `announcedInboundPeers` /
`spawned` on our own `close_peer` / `drop_all_links`, not only on the server
disconnect callback. Coordinate with HV-93/HV-94.

**Phase C.**
- **HV-97 → ✅** — `dropAllLinks()` FFI + snippet RPC shipped and verified
  (drops links, triggers a ~1 s reconnect); `_make_session_dir` keyed off
  `$IRIS_BENCH_RUN`; `stopAdvertising` now closes the GATT server. The
  back-to-back-tests-in-one-process fix is partially verified (GATT server now
  closed on teardown) — full re-verification is folded into HV-98.
- **HV-15 → 🟢** — the targeted reconnect is now hardware-proven:
  `ble.drop_all_links` → `connectGatt: initiated` 65 ms later →
  `discovery.connect_ok elapsed_ms ≈ 1100` on both phones (was: wait for the
  beacon to re-enter the scan buffer, up to 30 s). The end-to-end *message*
  recovery is blocked downstream by HV-98 (receiver-side), not by HV-15.
- **HV-11 → ✅** (unchanged — still no L3 trigger for a scan refusal).

### Session 12 closeout
- Devices: P1 = vivo V2205 (Android 15), P2 = vivo 2004 (Android 13). BLE only.
  **HCI snoop confirmed unavailable** on Funtouch OS without root.
- Findings advanced: **HV-97 ✅**, **HV-15 🟢** (reconnect speed proven).
  🟢 count 12 → 13.
- Tier 1: 22 findings — 7 🟢 (HV-90/91/92/93/7/14/15), 2 ✅ (HV-11/97… HV-97 is
  really Tier-0 harness), 1 🔒 (HV-8), 3 🔬 (HV-96/97-followup/98), 9 ⬜.
- New: `DiscoveryManager::wake()` (interruptible scan loop — also serves the
  RETRY button, HV-31/HV-33); `iris_bench` `test_forced_drop_reconnect_10x`;
  `Transport::drop_all_links()`.
- **Next session: HV-98** (inbound poller re-attach after reconnect — it blocks
  every drop-recovery scenario and the §2 30-min gate). Then HV-10 + HV-31
  (advertising resilience §5 group). HV-96 stays blocked (no HCI snoop).

---

## Session 13 — 2026-09-03 · P1 = vivo V2205 (Android 15) · P2 = vivo 2004 (Android 13)

Resumed on `main` @ 4b58b6a — 13 `integration` commits landed on top of Session 12
(CROSS-001..008; the relevant one is `9bddad9` which changed the Kotlin
`IRIS_SERVICE_UUID` `3e5c…0c0d` → `01000000-…` to byte-match the Rust core, and
`24e1c3b` renamed a couple of Kotlin error types). Verified the tree at HEAD:
`cargo build --workspace` ✓, iris-core **780**, iris-android **7**,
`sysval_fault_injection` **6** — all green. My HV-92/93/97 adapter code is intact.

### HV-98 — Inbound reassembly poller does not re-attach after a drop + reconnect

**Research note.**

**1. Mechanism (Session-12 evidence + platform research).** After
`dropAllLinks()` (or any real drop), P1 and P2 re-establish the GATT link in
~1 s and P1's ATT frames physically reach P2's characteristic — P2 logs
`onCharacteristicWriteRequest matchesIris=true len=300` — but **no
`msg.delivered`**. Chain:
- P2's `AndroidBleTransportAdapter.gattServerCallback.onConnectionStateChange`
  **never fires** for P1's connection dropping. This is a real platform gap:
  "when the client drops off the server has no idea" (Nordic DevZone; MIT App
  Inventor BLE keep-alive thread) — the server-side disconnect callback is
  unreliable on Android and OEM stacks (also the root of HV-93/HV-94).
- So P2's `announcedInboundPeers` (a *fire-once* `MutableSet<ULong>`, cleared
  only in `onConnectionStateChange(STATE_DISCONNECTED)` which never comes)
  keeps P1's stale `deviceHash`. `onCharacteristicWriteRequest`'s
  `if (announcedInboundPeers.add(h))` is false → **no re-announce** to
  `pendingAcceptedConnections`.
- On the Rust side, `ble.rs::ensure_accept_poller` keeps a **task-local**
  `spawned: HashSet<GattHandle>` (line 929). `close_peer` (line 1006) and
  `drop_all_links` run on a different task and cannot touch it. So when P2's
  own `drop_all_links` → `close_peer` aborts the inbound reassembly poller
  (`pollers.remove(pid)` → `abort()`), the accept-poller still has the handle
  in `spawned` and **never re-spawns** `spawn_inbound_poller`.
- Result: P1's frames land in `pendingGattWrites` on P2 and nothing drains
  them.

**2. Why the current code gets it wrong.** Two fire-once caches assume "we saw
this connection start, so a poller exists forever": Kotlin's
`announcedInboundPeers` and Rust's task-local `spawned`. Both are only cleared
by the OEM server-disconnect callback, which the platform does not reliably
deliver. Neither is cleared by *our own* teardown (`close_peer` /
`drop_all_links`).

**3. How it's solved elsewhere.** The consistent advice: drive the receive path
off *observed traffic*, not a one-time connect event. MIT App Inventor / Nordic
threads: the client sends a keep-alive so the server knows to re-advertise;
blessed-android / Nordic re-key the connection state on every `onConnectionState
Change` *and* treat a fresh characteristic operation as liveness. Martijn van
Welie pt.2: never trust one callback — always `close()` + reset on any state
transition and re-derive from there.

**4. IRIS constraints.** FFI seam: `pendingAcceptedConnections` is the only
Kotlin→Rust channel for "a peer is connected to our server"; re-announcing on it
is cheap (bounded queue, drained every 500 ms, Rust de-dups). No MAC-as-identity
change — the announce carries `deviceHash` + address exactly as today. The
accept-poller's `spawned` set must become shared so `close_peer` can prune it.

**5. Research outcome.** Two coordinated changes, one commit:
- **Kotlin** — `announcedInboundPeers` (fire-once) → `lastInboundWriteAtMs:
  ConcurrentHashMap<ULong, Long>`. `onCharacteristicWriteRequest` re-announces
  the accepted connection when the gap since this device's last IRIS write is
  `> INBOUND_REANNOUNCE_GAP_MS` (3 s) — a gap that large means the connection
  churned; during active messaging (sub-second writes) it never re-announces.
  Cleared on `stopAdvertising` (with the GATT server).
- **Rust** — `ensure_accept_poller`'s `spawned` set becomes
  `Arc<Mutex<HashSet<GattHandle>>>` on `BleTransport`; `close_peer` (and thus
  `drop_all_links` / `shutdown`) removes the peer's handle from it before
  dropping the connection entry, so a subsequent re-announce actually
  re-spawns `spawn_inbound_poller`.
Rejected: making the peripheral send a keep-alive (adds wire traffic + a
timer; the write-gap heuristic gets the same signal for free). Rejected:
polling `accepted_connections()` for "is this handle still alive" (the adapter
has no such query — the server callback is exactly what's missing).

**Phase D — implementation.**
- `crates/iris-core/src/transport/ble.rs`:
  - `BleTransport.accept_spawned: Arc<Mutex<HashSet<GattHandle>>>` — the
    accept-poller's "already spawned" set, was task-local (unreachable from
    `close_peer`). `ensure_accept_poller` uses it; `close_peer` removes the
    peer's handle from it after dropping the connection entry; `shutdown`
    clears it.
  - `SimulatedBleAdapter` gained `accepted: Mutex<Vec<AcceptedConnection>>` +
    `inject_accepted_connection` / `clear_accepted` + an `accepted_connections()`
    override (the sim never modelled the HW-9 accept path before).
  - Sim test `hv98_inbound_poller_re_attaches_after_our_own_drop`: accept a
    connection → frame delivers → `drop_all_links()` → re-inject the accepted
    connection + a frame → **must deliver again**. iris-core **781**.
- `android/.../AndroidBleTransportAdapter.kt`:
  - `announcedInboundPeers` (fire-once `Set`, cleared only on the OEM server
    disconnect callback that never comes) → `lastInboundWriteAtMs:
    ConcurrentHashMap<ULong, Long>`. `onCharacteristicWriteRequest` re-announces
    the accepted connection when this is the first write OR the gap since the
    device's last write is `> INBOUND_REANNOUNCE_GAP_MS` (1500 ms — above the
    ~0.5 s spacing of active messaging, below the ≥1 s a drop+reconnect takes;
    a redundant re-announce is a harmless no-op on the Rust side). Cleared with
    the GATT server in `stopAdvertising`.
- Also fixed an unrelated build break on `main` (CROSS-007 left api.kt error
  aliases stale) — commit 4df67d1.

**Phase T / I.**
- ❌ Attempt 1 (time-gated re-announce, `> 1500 ms` write-gap): drop 1/10
  **recovered in 3.4 s** — `onCharacteristicWriteRequest: (re)announcing inbound
  peer … (gap=3511ms)` → `msg.delivered` — the mechanism works. But drop 2/10
  failed (21 s): when the next drop came before P1's write cadence produced a
  >1.5 s gap, the re-announce did not fire and the (now-empty) `accept_spawned`
  entry was never re-populated. The gap heuristic is fragile under back-to-back
  drops / a fast-retrying sender.
- Attempt 2: **announce the accepted connection on EVERY IRIS write** (drop the
  gap heuristic). The Rust accept-poller de-dups a still-polled handle
  (`accept_spawned`), so a redundant announce is a cheap no-op; only a handle
  that `close_peer` cleared re-spawns. `pendingAcceptedConnections` is a bounded
  queue drained every 500 ms — during a 300-char ping-pong that is ~1 dup entry
  per drain, negligible.

**Phase C.** HV-98 → 🟢.
- `iris_bench --test_module tier1 --tests test_forced_drop_reconnect_10x` (BT-cycled
  both phones first): **10/10** forced `dropAllLinks()` cycles recovered,
  recoveries `[3.3, 3.7, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5, 3.5]` s,
  `failed={'P1': 0, 'P2': 0}` — every drop re-establishes the link AND the
  inbound reassembly poller, message delivers end-to-end. Evidence:
  `evidence/session-13-hv98/` + `evidence/mobly_logs/iris_2phone/latest`
  (`test_forced_drop_reconnect_10x-…` PASS; P2 logcat: 34 announce/deliver lines,
  12 `msg.delivered`).
- iris-core **781** / iris-android **0** green, `cargo build --workspace` clean,
  `Cargo.lock` reverted.
- Commit `<pending>` (ble.rs `accept_spawned` + sim accept path + hv98 sim test;
  AndroidBleTransportAdapter.kt announce-every-write; rebuilt jniLibs).

### Session 13 closeout
- **Devices:** P1 = vivo V2205 (Android 15, API 35, `10BCA20F4M000BB`);
  P2 = vivo 2004 (Android 13, API 33, `b2fbcd39`).
- **Advanced:** HV-98 🔬 → 🟢. Tier 1 now 8 🟢 / 2 ✅ / 1 🔬 / 1 🔒 / 10 ⬜.
  Total 🟢 13 → 14.
- **What changed in the real world:** before Session 12–13, any BLE link drop
  (RF glitch, `dropAllLinks`, OEM stack hiccup) left the *receiving* phone
  permanently deaf on that peer — the radio reconnected, ATT frames arrived at
  the characteristic, but nothing was reassembled or delivered; only a full app
  restart recovered it. Now the receiver re-attaches its inbound poller on the
  first frame after any reconnect, so a dropped link self-heals in ~3.5 s with
  no lost message.
- **Next session pickup:** HV-10 + HV-31 (§5 advertising-resilience group;
  HV-31 also benefits from `DiscoveryManager::wake()`).
- **Still owed:** HV-97 follow-up — re-verify "two test methods in one process"
  now that the inbound-poller lifecycle is fixed. §2 30-minute zero-loss session
  (partly gated on HV-96).
- **Still blocked:** HV-96 (spontaneous drop under load — needs HCI/btsnoop,
  impossible on vivo Funtouch without root). HV-8 🔒 (asymmetric-link MTU).
