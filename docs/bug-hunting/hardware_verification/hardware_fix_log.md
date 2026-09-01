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

**Phase C.** Commit _pending_. Tracker: new **HV-81 → `✅ Fixed`** under Tier 0.

---
