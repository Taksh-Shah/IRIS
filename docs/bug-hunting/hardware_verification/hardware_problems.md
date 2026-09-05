# Hardware Verification & Enhancement — Problem Tracker

**Owner:** Taksh Shah (hardware / two-phone bench)
**Scope:** everything that has to be true for two (and then three) physical Android
phones to *reliably* exchange messages over IRIS — BLE, Wi-Fi Direct, Wi-Fi Aware,
the FFI seam, multi-hop relay, transport selection/coexistence, the Android shell
UX, connection lifecycle, and the internet/TCP path (which is not wired on Android
at all).
**Companion documents:**
- [`hardware_problems_loop.md`](hardware_problems_loop.md) — **logic**: the
  research → implement → test-on-hardware → iterate → close protocol every finding
  must pass through before it is marked `🟢`.
- [`hardware_fix_log.md`](hardware_fix_log.md) — **journal**: one entry per bench
  session — devices used, `adb logcat` evidence, what landed, what regressed.

**Bench:** 2× physical Android phones available now (per operator); a 3rd is
required for multi-hop and is called out where it blocks a finding. All prior
"BLE/Wi-Fi Direct confirmed working" claims in `docs/` were made on 2–3 devices in
a single session and are **not** treated as durable here — every finding is
re-verified from scratch on the current build.

---

## Why this tracker exists (read this first)

The 6-section bug-hunt (`README.md`) fixed ~455 defects and demonstrated, once, on
a bench, that a BLE 3-way message and a Wi-Fi Direct message could be delivered.
The operator's field experience since then is: **it works sometimes and fails
sometimes**, in every dimension —

- a send succeeds, then the identical send moments later does not;
- a message goes over BLE once, then never again that session;
- the link establishes, carries one message, then is lost — and the *reply* fails
  seconds after a successful receive on the same link;
- discovery finds exactly one peer and fixates on it; the mesh never widens;
- Wi-Fi Direct group formation collides when both phones try to be group owner;
- advertising/scanning silently stops after a few reconnect cycles;
- there is no internet path at all — the app cannot use TCP/IP even with full
  connectivity;
- multi-hop, flood, and PRoPHET have **never** run on real radios;
- the UI hides the text you are typing behind the composer, has no send button,
  and demands a pasted 64-hex PeerId for every single message with no contacts,
  no nicknames, no reply affordance, and unicast only.

The root pattern from the prior hunt still holds: **the simulated adapters satisfy
the Rust contracts by construction, so CI is green while the real radios are
not.** Every finding below is either (a) a hardware behaviour the simulator cannot
model, (b) a race/timing defect that only manifests under real RF latency and
jitter, or (c) a product gap that makes the working parts unusable in the field.

This tracker's definition of done is **not** "the code compiles and the sim test
passes." It is: *the operator can pick up two phones cold, and every one of the
scenarios above works on the first try, ten times in a row, and the failure modes
that remain are understood and logged.*

---

## Progress Tracker

| Tier | Theme | Total | 🟢 HW-verified | ✅ Fixed (HW pending) | 🔬 Under research | 🔒 Blocked | ⬜ Not started |
|---|---|---:|---:|---:|---:|---:|---:|
| 0 | Test-integrity & instrumentation — you cannot fix what you cannot see | 15 | 6 | 7 | 0 | 1 | 1 |
| 1 | BLE single-hop reliability (the "sometimes works" core) | 28 | 18 | 6 | 1 | 1 | 2 |
| 2 | Wi-Fi Direct reliability & group-owner conflict | 9 | 0 | 4 | 0 | 0 | 5 |
| 3 | Connection lifecycle — drop, backoff lockout, auto-reconnect, coexistence | 10 | 5 | 3 | 0 | 0 | 2 |
| 4 | Multi-hop, relay, flood, PRoPHET — never run on hardware | 7 | 0 | 0 | 0 | 0 | 7 |
| 5 | Internet / TCP-IP transport — absent on Android | 4 | 0 | 0 | 0 | 0 | 4 |
| 6 | Transport selection & concurrent-radio coexistence | 5 | 0 | 1 | 0 | 0 | 4 |
| 7 | Wi-Fi Aware data path (NDP responder) | 3 | 0 | 0 | 0 | 0 | 3 |
| 8 | Shell UX — composer, contacts, addressing, reply, status | 11 | 2 | 1 | 0 | 0 | 8 |
| 9 | Additional findings from the methodology/internet-research pass | 17 | 0 | 2 | 0 | 0 | 15 |
| **Total** | | **109** | **31** | **24** | **1** | **2** | **51** |

**Last updated:** 2026-09-05 (Session 22, continued — **Tier 8: HV-54 + HV-55
closed, 🟢 HW-verified.** Both are UI-only fixes, independent of the Tier-2
radio blocker, worked in parallel per §2's explicit allowance. HV-54: the
floating composer/palette column now measures its own height and feeds it
into the transcript's bottom padding, so a growing multi-line composer never
covers unread messages. HV-55: an explicit send button next to the text
field, wired to the same `onSubmit` as the IME Send key, for keyboards/
input methods/accessibility tools where the IME action doesn't fire. Both
verified via `adb shell input` + screenshots on P1.)
· Previously: 2026-09-05 (Session 22 — **Tier 2 started, gate BLOCKED
(not closed).** HV-19 (deterministic GO/GC election via PeerId comparison),
HV-20 (persistent-mode credentials on the band-constrained path), and HV-22
(lost-group detection + re-formation via a `poll_health` tick) all
implemented and L1-verified (27 `wifi_direct` tests, 796 total `iris-core`),
but the Tier-2 hardware gate cannot be met: Wi-Fi Direct DNS-SD discovery
finds zero peers on both bench phones regardless of radio state (off→on
enabled, then a full radio power-cycle — neither fixed it). Root cause
attributed to HV-21 (PTR/TXT listeners never fire on either phone — fresh
evidence this session) rather than HV-25 (concurrency evidence this session
argues against it being the sole cause) or stale persisted P2P groups (a
reflection-based clear attempt was blocked by the platform withholding
`requestPersistentGroupInfo` data from non-privileged apps; reverted, not
committed). See the `## Tier 2` gate note for full detail. Per the tier
gate, do **not** proceed to Tier 6 until this is resolved or the operator
explicitly re-prioritizes — continuing into HV-21's own fix sketch next.)
· Previously: 2026-09-04 (Session 21 — **HV-34 + HV-101 + HV-104 closed,
all 🟢 HW-verified.** HV-34: decided **no OS bonding** (research showed it does
not help reconnect speed/reliability); built a persisted app-level "known
peers" list instead (`test_known_peers_survive_engine_restart` PASS 3/3).
HV-101: the "autoConnect=true is faster" premise was wrong per its own cited
source — kept `autoConnect=false`, added `TRANSPORT_LE`. HV-104: consolidated
N per-peer BLE inbound pollers into one transport-wide poller (part A; the
battery-rig half stays deferred). RETRY improved to 0.3–0.5 s (was 0.4–32.8 s).
Not proceeding to Tier 2 yet per the operator.)
· Previously: 2026-09-04 (Session 20 — **Tier 3 CLOSED; Tier-3→Tier-2 gate
MET.** The whole BLE teardown-then-reconnect path had silently regressed to a
permanent stall (steady-state fine, so short runs missed it). Three coordinated
fixes: **HV-105** (keep the MAC→candidate hint across `close_peer`), **HV-107**
(inbound GATT handles get their own map, not `connections` — so a node dials back
out after a peer restart), **HV-109** (one process-wide GATT server, never closed
on a mesh restart — no `serverIf` leak, the peer that kept its link is still
served). Hardware: RETRY 5/5, forced-drop 10/10, permission-revoke 3/3, BT-toggle
3/3 (< 60 s), Doze PASS. Also promoted 🟢: HV-15, HV-28, HV-29, HV-30, HV-31,
HV-99, HV-106, HV-12, HV-16. Next: Tier 2 (Wi-Fi Direct — Phase-R note written
Session 19). Follow-ups: HV-99 sub-30 s BT-toggle, HV-34 (bonding — needs the
operator's decision), HV-101/HV-104.)
· Previously: 2026-09-04 (Session 17 — HV-33 + HV-29 ✅ HW-PENDING; the §2
30-minute zero-loss session PASSED (150/150 both ways) → Tier-1→2 gate met.)
· Previously: 2026-09-03 (Session 16 — **HV-95 ✅ HW-PENDING** —
`ensureGattServer` now retries + returns a bool; `startAdvertising` fails loudly
if the GATT server won't open (no more "connectable beacon with no server");
core `ble.advertise_watchdog` re-drives advertising every 5 s if it's down.
Delivery smoke 10/10, 0 spurious watchdog. Candidate **HV-100** opened
(iris-core suite flakiness).) · Previously: 2026-09-03 (Session 15 — **HV-94 HW-verified** (client GATT
disconnect surfaced to the core via `drainDisconnectedHandles`; peer app
process dies → torn down + reconnected <1 s, 0 lost msgs;
`test_peer_process_restart_recovers` 3/3), **HV-16 ✅** (cross-FFI UUID
regression guard: Rust `hv16_…` + Kotlin `BleUuidParityTest`), **HV-12 ✅
HW-PENDING** (`RSSI_FLOOR_DBM` -85 → -95, single source; ping-pong 10/10 no
regression), **HV-13 ✅ HW-PENDING** (`known_addresses` TTL-swept + capped +
evicted on `close_peer`; part-a split-id reconciliation deferred). Also this
session: HV-99 partial, HV-31 blocked, HV-10 HW-pending, HV-98 verified.) ·
Previously: 2026-09-03 (Session 14 — **HV-10 ✅ HW-PENDING** (bounded
advertising-retry backoff — no bench trigger for `TOO_MANY_ADVERTISERS`) +
**HV-31 🔒 Blocked** (§5 group; `btStateReceiver` for
`BluetoothAdapter.ACTION_STATE_CHANGED` + `BleAdapter::drain_adapter_events()`
land and are a strict improvement, but the 10/10 recovery bar is blocked on new
**HV-99** — `connectGatt`'s 30 s reconnect-probe timeout — and a
`DiscoveryManager::wake()` path for FFI-surfaced transport events; 3 hardware
attempts, blocker analysis in `hardware_fix_log.md`). Commit 0875136. New
candidate **HV-99** opened. Next: HV-94 (zombie GATT link).) · Previously:
2026-09-03 (Session 13 — **HV-98 🟢** — inbound reassembly
poller now re-attaches after a drop + reconnect: announce-every-write on the
Kotlin side, `accept_spawned` shared set cleared by `close_peer` on the Rust
side. `iris_bench test_forced_drop_reconnect_10x` **10/10**, recovery ~3.5 s
each, 0 `msg.delivery_failed`. End-to-end message recovery after HV-97's
`dropAllLinks()` is now fully closed. Next: HV-10 + HV-31 advertising
resilience. HV-96 stays blocked.) · Previously: 2026-09-02 (Session 12 — **HV-97 ✅** (`dropAllLinks()` hook +
`DiscoveryManager::wake()` interruptible scan loop + per-run evidence dir +
`stopAdvertising` closes the GATT server), **HV-15 🟢** — forced-drop bench:
`ble.drop_all_links` → `connectGatt` 65 ms later → `connect_ok elapsed_ms ≈
1100` on both phones (was up to 30 s). End-to-end message recovery blocked
downstream by new **HV-98** (inbound poller does not re-attach after reconnect —
receiver side, HV-93/HV-94 family). HCI snoop confirmed unavailable on Funtouch
OS without root — HV-96 stays blocked). · **Active tier:** 1

Legend: `🟢` verified on ≥2 physical phones with logged evidence · `✅` code fix
landed, hardware verification still owed · `🔬` in the research phase (see loop
§2) · `🔒` blocked (needs a 3rd device / a cross-section contract / hardware not
on hand) · `⬜` untouched.

---

## Tier 0 — Test-integrity & instrumentation

*Rationale: every later tier's "is it fixed?" question is unanswerable until the
bench can observe what the radios actually do. Do this tier first.*

### HV-1 — The Android engine's own unit tests do not compile against the real constructor

- **Fix status:** ✅ Fixed · commit 9a0400b · 2026-09-01 · CI-verified (no HW
  gate) · see `hardware_fix_log.md` Session 01
- **Area:** `crates/iris-android/src/engine.rs` (test module, ~line 570+)
- **Severity:** High (test-integrity) · **HW gate:** none (host build)

**Resolution:** split `IrisEngine::new` into the FFI constructor + a private
`build(…, crypto: Arc<dyn CryptoProvider>)`; added `#[cfg(test)]`
`new_for_test` (inert `DevCryptoProvider`) + `register_test_key` (PRY-33 fix).
5 test call sites updated; `round_trip_delivers_over_shared_mesh` runs again.
`cargo test -p iris-android` → 6/6, now an explicit named CI step.

**What:** `IrisEngine::new` takes five parameters
`(ble, aware, direct, node_id, signer)`. Every call in the `#[cfg(test)] mod
tests` block passes **four** — no `signer` (e.g.
`IrisEngine::new(Arc::new(SimBle), Arc::new(SimAware::default()), Arc::new(SimDirect), vec![7u8; 32])`).
The Android engine test module cannot compile, so `round_trip_delivers_over_shared_mesh`
— the one test that supposedly proves an end-to-end send through the FFI bridge —
has not run since `signer` was added.

**Why it matters:** the single most load-bearing Android test is dead. Any claim
that "the engine round-trips a message" rests on a test that does not build.

**Fix sketch:** add a `SimSigner` (or reuse `DevCryptoProvider`-style stub) and
thread it through all `IrisEngine::new` call sites in the test module; wire it in
CI (`cargo test -p iris-android`) so it cannot rot again.

**HW verification:** n/a — but this unblocks the sim round-trip that every BLE/WD
finding regression-checks against.

---

### HV-2 — CI has no Android instrumented-test leg; the sim is the only thing exercised

- **Fix status:** 🟢 HW-verified · commits c0bfb31 (HV-89 interim) + 0519516 ·
  2026-09-01 · the `iris_bench` harness drives P1=V2205 + P2=vivo 2004 end to
  end and P1→P2 BLE delivery works 9/10 · Session 06. **The harness (loop §4.2
  deliverable) is done.** The §2 Tier-0→Tier-1 *gate* — the smoke at **10/10** —
  is not yet met: iter 1 fails because the first GATT connection takes ~30 s and
  the queued message exhausts its retry budget before the link is up (→ HV-90 +
  a retry-hold, the first Tier-1 work).
- **Area:** `iris_bench/` (host Python), `android/app/src/androidTest/`
  (`IrisSnippet`), `android/app/build.gradle.kts`,
  `docs/bug-hunting/hardware_verification/evidence/`
- **Severity:** High (test-integrity) · **HW gate:** 2 phones ✓

**Delivered:** `iris_bench` (Mobly) — `IrisBenchBase` (registers 2 controllers,
force-installs the app + snippet APKs, grants perms, loads the snippet, per-test
evidence), `test_tier0_smoke.py`, `evidence.py` (logcat + `dumpsys
bluetooth_manager`/`wifip2p` + `meshSnapshot()` JSON + best-effort `btsnoop` via
`bugreport`). The `IrisTestSnippet` APK (`org.iris.mesh.test`, mobly-snippet-lib
1.4.0) builds a real `IrisEngine` over the real adapters and exposes `startMesh`
/ `stopMesh` / `nodeId` / `staticX25519` / `registerPeerKey` / `sendText` /
`awaitDelivered` / `meshSnapshot`. First run: `evidence/session-01/`.
**Still owed:** the CI L1+L2 gate + the bindgen/`.so` regen-diff check
(HV-88 recurrence) + rewriting the stale `docs/testing/ANDROID_BUILD_STATUS.md`.

**What:** CI builds `libiriscode.so` and runs the Rust suite. There is no
`connectedAndroidTest`, no emulator leg, no `adb`-driven smoke. The 66 historical
Kotlin compile errors (ANDROID_BUILD_STATUS.md) were only found when someone ran a
real `./gradlew assembleDebug` by hand. The Kotlin adapters — the code that
actually talks to the radios — have **zero** automated coverage.

**Why it matters:** every adapter fix in this tracker is one refactor away from
silently breaking, and nobody would know until the next manual bench session.

**Fix sketch:** (a) add `./gradlew :app:assembleDebug :app:testDebugUnitTest` to
CI as a required gate (catches the compile-error class); (b) add a Robolectric or
JVM-level test of `FrameCodec` / handshake framing / `peerHandleFor` idempotency
(the pure-logic bugs); (c) **build the Mobly `iris_bench` multi-device harness**
— an `IrisTestSnippet` APK (Mobly Snippet Lib) exposing `startMesh` /
`meshSnapshot` / `sendText` / `awaitDelivered` / `nodeId` / fault-injection hooks,
plus a host-side Python suite that drives two (later three) phones from a laptop
with a structured pass/fail + `btsnoop`/`logcat`/`dumpsys` evidence pipeline. This
is the standing hardware-test tool — see the loop spec §4 for the full design. It
replaces every ad-hoc `adb`/manual-tap workflow.

---

### HV-3 — No structured, greppable on-device diagnostic surface

- **Fix status:** 🟢 HW-verified · commit a7daad0 · 2026-09-01 · `/diag` fires
  on P1=V2205 + P2=vivo 2004, structured `iris.diag` logcat + console block ·
  Session 03. **Partial:** the `snapshot()` FFI + `/diag` command are done
  (`FfiMeshSnapshot`: node id, per-transport id/state/cost, `NeighborTable`
  neighbours + links, `MessageEngine` counters); the loop-§(a) `iris.*` tag
  rename and §(c) 100-event ring are **not** — see follow-up **HV-86**.
- **Area:** `crates/iris-android/src/engine.rs` (`snapshot()` + `Ffi*` records),
  `kotlin/.../iriscode/{api.kt,uniffi/.../iriscode.kt}`,
  `android/.../data/MeshRepository.kt`, `.../command/{CommandRegistry,CommandExecutor}.kt`,
  `.../ui/MeshViewModel.kt`, `crates/iris-desktop/ui/commands.js`
- **Severity:** High (instrumentation) · **HW gate:** verify on device ✓

**What:** field debugging today means `adb logcat | grep -E 'IrisBle|IrisWifiDirectDiag|IrisBleDiag'`
across two phones and eyeballing interleaved lines. The tags are ad-hoc
(`IrisBle`, `IrisBleDiag`, `IrisWifiDirectDiag`, `IrisMeshRepository`), the levels
are inconsistent (`Log.d` for things that matter, `Log.w` for normal conditions
like "no NAN hardware"), and there is no single "what is the mesh doing right
now" readout: transports registered, per-transport state, live GATT handles, live
Wi-Fi Direct group, discovered peers, connection attempts, last N send outcomes,
scan-backoff state.

**Why it matters:** "sometimes works" is only diagnosable if you can see, at the
moment of failure, which transport was selected, whether a link existed, and why
the send returned what it did. Right now you cannot.

**Fix sketch:** (a) one tag prefix `iris.*` with consistent levels
(`iris.ble.scan`, `iris.wd.group`, `iris.route.select`, `iris.msg.send`); (b) a
`snapshot()` FFI call returning a struct the shell renders on `/diag` and also
dumps to logcat on demand; (c) a rolling in-memory ring of the last 100
transport-level events retrievable over FFI so a failure can be inspected after
the fact. Base it on the existing `MetricsRegistry` and `observability` module.

---

### HV-4 — The simulated adapters model success, not RF

- **Fix status:** ✅ Fixed · commit 1ad395d · 2026-09-01 · CI sim model, no HW
  gate · Session 04
- **Area:** `SimConfig` / `SimulatedTransport` (`crates/iris-core/src/transport/simulated.rs`),
  `crates/iris-core/tests/sysval_fault_injection.rs`
- **Severity:** Medium (test-integrity) · **HW gate:** none

**Resolution:** four seeded, deterministic failure modes on `SimConfig`
(`disconnect_after_sends`, `mtu_shrink_after_sends`+`mtu_after_shrink`,
`scan_fail_after_calls`, `busy_first_n_connects`), each off at its zero default,
exercised in `send()`/`discover_peers()`/`connect()`. 5 unit tests + 5
`sysval_fault_injection` scenarios. GO/GO election tie deferred to HV-19 (the
Wi-Fi Direct sim has no group election). Later Tier-1..3 findings extend this
model per loop §1 Phase D step 4.

**What:** the sim adapters deliver every frame, in order, with zero latency, no
loss, no MTU renegotiation mid-stream, no disconnect, no `onScanFailed`, no
`WifiP2p reason=BUSY`, no channel-disconnect, no GO/GO collision. Every hardware
bug in the prior hunt (HW-1..21, FFI-1..19, BLE-1/2) was invisible to them by
construction.

**Why it matters:** a fix "verified by the sim round-trip" proves the Rust
plumbing, not the behaviour under real RF. Tier 1–4 fixes need a harness that can
inject the failure modes.

**Fix sketch:** extend the sim adapters with an injectable fault model — per-frame
loss probability, latency distribution, mid-stream MTU change, unsolicited
disconnect, `onScanFailed` after N restarts, `BUSY` on the first K action calls,
a GO-election tie. Add `sysval_*` scenarios that assert IRIS recovers. This does
not replace the phone bench; it makes the phone bench's findings reproducible in
CI.

---

### HV-5 — `received_at_ms` doc comment still says "origination", code says local receipt

- **Fix status:** ✅ Fixed · commit 5fdaeeb · 2026-09-01 · doc/comment only, no
  HW gate · Session 01
- **Area:** `crates/iris-android/src/engine.rs:50` (doc) vs `:351` (code)
- **Severity:** Low (doc/code drift, but it's on the FFI contract) · **HW gate:** none

**What:** GAP-9 correctly changed the field to `unix_now().saturating_mul(1000)`
(local receipt), but the `FfiIncomingMessage.received_at_ms` doc comment still
reads "Unix epoch milliseconds at origination." The Kotlin side
(`MeshRepository.toUi`) treats it as `receivedAtMs` for sort order.

**Fix sketch:** fix the comment; add a `originated_at_ms` field if the UI ever
needs to show message age (it currently cannot distinguish a 1-hour-old relayed
message from a fresh one).

---

### HV-6 — No field-KPI capture: delivery rate, hop count, link lifetime are not measured on device

- **Fix status:** 🟢 HW-verified · commit 49f0d99 · 2026-09-01 · `/stats` +
  `iris.kpi` logcat line on P1=V2205 + P2=vivo 2004 · Session 04. **Partial:**
  message/route/security counters are exposed; per-link lifetime, scan-restart
  rate and group-formation success rate need HV-27 (per-link health) + HV-86
  (event ring).
- **Area:** `crates/iris-android/src/engine.rs` (`FfiCounter`, `snapshot().counters`,
  `iris.kpi`), `android/.../ui/MeshViewModel.kt` (`/stats`),
  `.../command/*`, `crates/iris-desktop/ui/commands.js`
- **Severity:** Medium (instrumentation) · **HW gate:** verify on device ✓

**What:** `docs/performance/*` calls the throughput/latency numbers
"folklore-grade" (SYSTEM_TEST_REPORT §5). There is no on-device counter for:
messages sent / delivered / expired-in-queue / relayed, per transport; median
GATT link lifetime before drop; scan-restart count per hour; group-formation
success rate. The operator's "it fails sometimes" cannot be turned into "it fails
23% of the time, always after a scan-backoff event" without these.

**Fix sketch:** wire the existing `MetricsRegistry` counters through the FFI
`snapshot()` from HV-3 and render a `/stats` block. Log a one-line CSV row per
send outcome so a bench session produces an analysable file.

---

### HV-81 — `iris-storage` does not build; TLS/NoTls `Connection` type mismatch blocks the Phase-D build gate

- **Fix status:** ✅ Fixed · commit e044bda · 2026-09-01 · host build, no HW
  gate · Session 02
- **Area:** `crates/iris-storage/src/pg.rs` (`establish`),
  `crates/iris-storage/src/seal.rs` (test literal)
- **Severity:** High (blocks `cargo build --workspace --all-features` and
  `cargo test -p iris-storage` — both loop Phase-D gates)

**What (found in Session 01):** TAK-6's `SslMode` split made `establish()`'s
`if *ssl_mode == Require { pg.connect(tls) } else { pg.connect(NoTls) }` return
two different `Connection<Socket, S>` types (`RustlsStream<Socket>` vs
`NoTlsStream`) — `E0308`. The `ssl_mode` field was also never added to the
`PgStorageConfig` test literal in `seal.rs` (`E0063`). Pre-existing at `fc720d5`;
not from the hardware deep-dive but on the critical path for every later finding.

**Resolution:** generic `spawn_pg_driver<S>()` helper; each `if`/`else` arm
spawns its own driver and yields only `Client`. `ssl_mode: SslMode::Require`
added to the test literal. `cargo build --workspace --all-features` and
`cargo test -p iris-storage` both green.

---

### HV-82 — `iris-core` ROUT-25 cold-start-spray tests fail at HEAD

- **Fix status:** ✅ Fixed · commit 6c0a755 · 2026-09-01 · sim-only, no HW
  gate · Session 02
- **Area:** `crates/iris-core/src/routing/opportunistic.rs`,
  `crates/iris-core/src/routing/mod.rs` (tests `rout25_cold_start_sprays_instead_of_flooding`,
  `rout25_spray_fallback_fires_when_cold_start`)
- **Severity:** Medium (test-integrity) · **HW gate:** none

**What:** two ROUT-25 tests failed on a clean `fc720d5`. `ca02ad6` (ROUT-23)
lowered `l_for_priority(P4+, _)` to 1 ("direct-only") after `5ffffb0` (ROUT-25)
wrote the tests against the old sprayable P4 budget — so a P4 cold-start now
correctly declines to spray and the assertions were stale.

**Resolution:** the two tests now use P2 (L=3, spray-eligible) to exercise the
real "cold start → spray not flood" intent; added two new pins
(`rout25_spray_fallback_direct_only_priority_does_not_spray`,
`rout25_direct_only_priority_cold_start_still_floods`) for the ROUT-23 × ROUT-25
interaction. `cargo test -p iris-core rout25` → 7/7.

---

### HV-83 — `sysval_mesh_integration::p0_multipath_includes_satellite_emergency_only` fails at HEAD

- **Fix status:** 🔒 Blocked — **deferred: satellite is post-v1 scope** (operator
  decision, Session 05). The satellite transport will not ship or be benched in
  this pass, so a test asserting P0-multipath satellite behaviour is not on the
  Tier-0 critical path. Revisit if/when satellite enters scope.
- **Area:** `crates/iris-core/tests/sysval_mesh_integration.rs:223`
- **Severity:** Low (test-integrity for an out-of-scope transport) · **HW gate:** none

**What:** the P0 multipath integration test (satellite included only for
emergency traffic) panics on a clean `fc720d5`. Not from the hardware deep-dive;
found while running the loop's narrow test gate for HV-82. Likely fallout from a
transport-selection / `RadioConflictGroup` change (cf. HV-46). It keeps
`cargo test -p iris-core` from being 100% green, but the failure is entirely in
the satellite path — everything the mesh tiers exercise is green.

---

### HV-84 — The debug APK has not built since 2026-08-30: stale UniFFI Kotlin bindings + missing `api.kt` facade alias

- **Fix status:** 🟢 HW-verified · commit 3daf0da · 2026-09-01 · APK builds,
  installs, and the engine starts on P1=V2205 + P2=vivo 2004 · Session 03
- **Area:** `kotlin/src/main/kotlin/iriscore/uniffi/iriscode/iriscode.kt`
  (generated), `kotlin/src/main/kotlin/iriscode/api.kt` (facade),
  `android/app/src/main/jniLibs/*/libiriscode.so`
- **Severity:** Critical (nothing downstream — HV-2, all of Tier 1+ — can run
  without an installable APK) · **HW gate:** APK install + engine start on 2 phones

**What:** `bde5c8e` ("AN-1 — replace DevCryptoProvider with real
AndroidCryptoProvider", 2026-08-30) added the `FfiCryptoSigner` uniffi trait on
the Rust side and `import iriscode.FfiCryptoSigner` + an anonymous impl in
`IrisCoreModule.kt`, but did **not**: (a) regenerate the committed
`uniffi/iriscode/iriscode.kt` bindings (still had no `FfiCryptoSigner`), nor
(b) add `typealias FfiCryptoSigner = uniffi.iriscode.FfiCryptoSigner` to the
`iriscode` facade `api.kt`. Result: `./gradlew :app:assembleDebug` fails at
`:app:kspDebugKotlin` — Hilt's `KspAggregatedDepsProcessor` reports
`'FfiCryptoSigner' could not be resolved` across every `IrisCoreModule`
provider. Same root cause as HV-1 (that commit broke several things at once).
`docs/testing/ANDROID_BUILD_STATUS.md` is also stale (describes a 66-error
state from an earlier build attempt).

**Also required (toolchain, one-time):** the machine had only JDK 25, which
Gradle 8.9 rejects ("What went wrong: 25.0.2"). Installed **Temurin JDK 21**
(`C:/Program Files/Eclipse Adoptium/jdk-21.0.12.101-hotspot`); gradle is invoked
with `-Dorg.gradle.java.home=<jdk21>` (there is no `gradlew` script — only
`gradle/wrapper/gradle-wrapper.jar`, run via
`java -cp gradle/wrapper/gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain`).
`Cargo.lock` was also stale — missing the `rustls`/`ring`/`aws-lc-rs` tree that
`iris-storage`'s TLS path pulls in — and is regenerated here.

**Resolution:** `uniffi-bindgen 0.31.2 generate --library target/debug/iriscode.dll
--language kotlin` → replaced `iriscode.kt`; added the `FfiCryptoSigner` alias to
`api.kt`; rebuilt `libiriscode.so` for all three ABIs via `cargo ndk`; committed
the refreshed `Cargo.lock`. `:app:assembleDebug` → **BUILD SUCCESSFUL**; APK
installs on both phones; app launches, loads the `.so`, engine starts, BLE comes
up (Wi-Fi Aware "not supported on this device" — expected, no NAN hardware;
Wi-Fi Direct hits `reason=2`/BUSY on P2 — that is HV-23, tracked separately).

---

### HV-85 — The Android JVM unit-test leg (`:app:testDebugUnitTest`) does not compile

- **Fix status:** ✅ Fixed · commit 13c5e2e · 2026-09-01 · host build · Session 03
- **Area:** `android/app/src/test/kotlin/iriscore/adapter/AdapterLifecycleTest.kt`
- **Severity:** High (test-integrity — the loop's L2 layer) · **HW gate:** none

**What:** `9431aca` (FFI-12, 2026-08-27) added `dispose` as `SessionGate`'s
second constructor parameter (with a default). Kotlin routes a trailing lambda
to the **last** parameter, so `SessionGate { … }` in the tests binds the lambda
to `dispose` and reports `create` missing —
`:app:compileDebugUnitTestKotlin` has failed since. Every production call site
already uses `SessionGate(create = { … })`.

**Resolution:** updated the 5 test call sites to the named-arg form.
`:app:testDebugUnitTest` → 77/77.

---

### HV-86 — HV-3 follow-up: `iris.*` tag rename + the 100-event transport ring

- **Fix status:** 🟢 HW-verified · commit 78beb32 · 2026-09-01 · `/diag` ring
  renders on P1=V2205 + P2=vivo 2004; `adb logcat -s iriscore` catches the
  adapter lines · Session 05
- **Area:** `crates/iris-core/src/observability/ring.rs` (new),
  `crates/iris-android/src/logging.rs`, `crates/iris-android/src/engine.rs`
  (`FfiLogEvent`, `snapshot().recent_events`),
  `android/.../util/IrisLog.kt` (new) + the 2 transport adapters,
  `android/.../ui/MeshViewModel.kt`
- **Severity:** Medium (instrumentation) · **HW gate:** verify on device ✓

**Resolution:** (b) `observability::ring::RingLayer` — a `tracing` Layer
capturing every `event = "..."` taxonomy event into a process-global 128-entry
ring, surfaced as `FfiMeshSnapshot.recent_events` and rendered as the tail of
`/diag`. (a) the 14 Kotlin adapter `Log.d("IrisBleDiag"/…)` calls route through
`IrisLog` → tag `iriscore`, prefix `iris.ble.*` / `iris.wd.*`. Adds
`tracing-subscriber` to iris-core deps (Appendix B).

**What:** HV-3's fix sketch had three parts; (b) `snapshot()`/`/diag` is done
(commit a7daad0). Still open: (a) one consistent `iris.*` tag/target scheme with
sane levels across every transport/discovery/engine log site (today: `IrisBle`,
`IrisBleDiag`, `IrisWifiDirectDiag`, `iriscode::engine`, `iris_core::*` — mixed),
and (c) a rolling in-memory ring of the last ~100 transport-level events
retrievable over FFI so a failure can be inspected after the fact. Lower urgency
than the live snapshot; fold into HV-6 (field-KPI capture) if convenient.

---

### HV-87 — `CapabilityBundle` test literal missing `dp_snapshot`

- **Fix status:** ✅ Fixed · commit 678dfe3 · 2026-09-01 · host build · Session 04
- **Area:** `crates/iris-core/src/discovery/neighbor_table.rs` (test literal)
- **Severity:** Medium (test-integrity) · **HW gate:** none

**What:** an upstream commit (PRoPHET DP-over-handshake) added
`dp_snapshot: Vec<(Vec<u8>, u16)>` to `CapabilityBundle` but left the literal in
`capabilities_recorded_and_retained` un-updated → `cargo test -p iris-core`
E0063. Added `dp_snapshot: Vec::new()`.

---

### HV-88 — Upstream AN-6 FFI wiring incomplete: APK crashed on launch

- **Fix status:** 🟢 HW-verified · commit 49f0d99 · 2026-09-01 · APK launches on
  P1=V2205 + P2=vivo 2004 without the `UnsatisfiedLinkError` · Session 04
- **Area:** `kotlin/.../iriscode/{api.kt, uniffi/.../iriscode.kt}`,
  `android/app/src/main/jniLibs/*/libiriscode.so`
- **Severity:** Critical (no runnable APK) · **HW gate:** APK launch on 2 phones

**What:** the AN-6 commit that landed in the 2026-09-01 pull added the
`FfiX25519KeyProvider` uniffi trait + `IrisEngine::new_with_x25519` and the
Kotlin `provideFfiX25519KeyProvider` / `IrisEngine.newWithX25519` call, but did
**not** regenerate the committed `iriscode.kt`, add the `api.kt` facade alias,
or rebuild the jniLibs `.so`. `:app:kspDebugKotlin` failed
(`'FfiX25519KeyProvider' could not be resolved`) and, once that was fixed, the
app FATAL-crashed at startup:
`UnsatisfiedLinkError: undefined symbol: uniffi_iriscode_fn_clone_ffix25519keyprovider`
(regenerated `iriscode.kt` expected symbols absent from the stale `.so`). Third
instance of this exact pattern (see HV-1, HV-84). **A CI gate that runs
`uniffi-bindgen generate` + `cargo ndk build` + `:app:assembleDebug` and fails on
any diff to the committed bindings/`.so` would catch the whole class** — fold
into HV-2.

**Resolution:** regenerated `iriscode.kt`, added the `FfiX25519KeyProvider`
alias, rebuilt `libiriscode.so` for all 3 ABIs.

---

### HV-100 — `iris-core` full-suite tests are intermittently flaky under parallelism

- **Fix status:** ⬜ (noticed Session 16)
- **Area:** `cargo test -p iris-core --lib` (test harness, not product code)
- **Severity:** Low (undermines "green tree after every commit") · **HW gate:** none

**What:** two separate problems in the `iris-core` test suite:
1. **Flaky:** `message_engine::tests::broadcast_is_delivered_and_also_relayed`
   fails ~3/5 full parallel `cargo test -p iris-core --lib` runs (Sessions
   16–17), always green on re-run and in isolation — a shared-resource / timing
   race under parallelism.
2. **Stale:** `tests/tokio_behavior.rs` —
   `message_engine_round_trip_delivers_under_paused_time` and
   `engine_send_abort_clean_when_dropped_under_paused_time` fail at HEAD with
   `Crypto(KeyUnavailable)`: PRY-33 / CROSS-003 made `seal_outbound` fail closed
   and the file's `dev_engine` helper never registers a recipient key (the lib
   tests' `alice_engine` does — `MemoryKeyDirectory` + `dir.insert(BOB, …)`).

**Fix sketch:** (1) run the suite ~15× to enumerate every flaky test; compare
default parallelism vs `--test-threads=1`; isolate the shared resource / pause
time. (2) give `tokio_behavior.rs::dev_engine` a `MemoryKeyDirectory` like
`alice_engine`. Do not paper over with retries or `#[ignore]`.

---

## Tier 1 — BLE single-hop reliability

*This is the "sometimes works, sometimes not" core. Two phones, one hop, one
message. It must be 10/10 before anything else is trusted.*

**§2 Tier-1→Tier-2 gate status (2026-09-04, Session 17):**
- HV-7 🟢 / HV-8 🔒-with-analysis · HV-10 ✅ / HV-11 ✅ (both: no bench trigger
  for their platform failure code — effectively 🔒-with-analysis) · HV-14 🟢 /
  HV-15 🟢.
- **30-minute two-phone session: PASS** — `test_30min_zero_loss_session`, 150
  rounds / 300 user messages, **fwd 150/150 + rev 150/150**, **0 `close_peer`,
  0 connect-backoff, 0 link drops** on either phone (`session-17-30min`).
- Residual: HV-102 (ACK path lossy under sustained load — 109 dupes / 2 failed
  ACKs, no user data lost). Not a link loss; land before calling Tier 1 fully
  done, but it does **not** hold the gate.
- **→ Tier 3 (lifecycle) is the next active tier per §2** (Tier 3 before Tier 2).

### HV-89 — Android has no key directory / key-exchange; since PRY-33 every addressed send fails closed

- **Fix status:** 🟡 Interim landed (commit c0bfb31) — `BenchKeyDirectory` +
  FFI `register_peer_key`; the `iris_bench` harness hand-feeds the two phones'
  keys and delivery then works. **The real fix is still owed:** handshake-carried
  `KeyAdvertisementV1` + `TrustKeyDirectory` (as desktop does) so any two IRIS
  nodes exchange keys automatically. Found Session 06 (HV-2 Step 0).
- **Area:** `crates/iris-android/src/engine.rs` (no `set_key_directory`),
  `crates/iris-core/src/discovery/{mod,handshake}.rs` (handshake carries no key
  ad), `crates/iris-core/src/identity/{advertise,trust_store}.rs` (the reusable
  pieces), `android/.../identity/X25519StaticAd.kt`
- **Severity:** Critical · **HW gate:** 2 phones (a message actually delivers)

**What:** `MessageEngine::seal_outbound` was made **fail-closed** by PRY-33 /
RED-0002 (commit `3d269d0`): an addressed unicast whose 32-byte recipient has no
X25519 key in the engine's `key_directory` returns
`MsgEngineError::Crypto(KeyUnavailable)` — it no longer falls through to
plaintext. The **Android engine never calls `set_key_directory`** (the only call
is the `#[cfg(test)] register_test_key` helper), and the discovery handshake
(`CapabilityBundle`) carries **no** `KeyAdvertisementV1` — so `key_directory` is
`None` forever and *every* real Android→Android message dies at `seal_outbound`
with `KeyUnavailable`. Confirmed live: `/to <peer>` + `/send` on P1 → the Android
`RelayOutbox` `QUEUED` chip, core counters stay zero. **The old "BLE 3-way
confirmed working" claim predates PRY-33** — back then addressed sends delivered
plaintext-signed.

**What exists to reuse:** `identity::advertise::KeyAdvertisementV1` (signed
`identity_pubkey → static_x25519_pubkey` binding), `TrustStore::adopt_advertisement`,
`TrustKeyDirectory` (a `KeyDirectory` over the trust store — desktop wires
exactly this in `engine_handle.rs:215`). AN-6 gave Android
`FfiX25519KeyProvider::static_public_key()`. Missing: (1) a `build_with_signer`
on `KeyAdvertisementV1` (Android's ed25519 key is in the Keystore, reached only
via `FfiCryptoSigner`), (2) a field on `CapabilityBundle` (or the first
post-connect frame) to carry the serialized ad, (3) `DiscoveryManager` adopting
received ads into a `TrustStore` + exposing `trust_key_directory()`, (4) the
Android engine calling `set_key_directory(...)`.

**Interim for the HV-2 smoke:** an FFI `register_peer_key(peer_hex, x25519_hex)`
+ snippet `staticX25519()` lets the harness wire keys out-of-band (the same
shape as HV-1's `register_test_key`), so the BLE single-hop delivery path can be
tested while the real handshake exchange is built.

---

### HV-90 — BLE GATT: the first connection takes ~30 s, and a message queued before the link is up is dropped

- **Fix status:** 🟢 HW-verified · commit 28d0f1a · 2026-09-01 · the iris_bench
  tier-0 smoke `test_p1_sends_p2_receives_10x` **PASSES 10/10** on P1=V2205 +
  P2=vivo 2004 (was 9/10) — **meets the §2 Tier-0→Tier-1 gate** · Session 07.
  The message-hold half is fixed; the ~30 s first-connect latency itself
  (part 1 below) is not — iter 1 still takes 27 s, it just no longer *loses*
  the message. Shrinking that window is follow-up **HV-91** (BLE
  discovery/connect cadence).
- **Area:** `AndroidBleTransportAdapter.connectGatt`, `ble.rs` connect path +
  `scan_allowed`/`DiscoveryManager` cadence, `MessageEngine` delivery-retry
  budget; interacts with HV-13, HV-14, HV-33
- **Severity:** Critical (the operator's "BLE sometimes works" — and the last
  thing between here and a 10/10 Tier-0 gate) · **HW gate:** 2 phones ✓

**What (harness evidence, `evidence/session-01`):** with `iris_bench` driving
both phones, BLE discovery + GATT connection + delivery **all work** —
`connectGatt: ready resolved successfully`, MTU negotiated to **517**,
`gattWrite … len=350` goes through, and messages 2–10 of the smoke deliver in
~200 ms each. Two real defects remain:
1. **First-connect latency ~30 s.** `startMesh` → first `GATT_Connect` ~33 s
   later, connected ~2 s after that. The delay is the discovery scan / connect
   cadence (candidate: HV-14 scan-restart backoff, or `connectGatt(autoConnect
   = false)` with a slow first direct connect, or the beacon-parse → connect
   ordering — the peer id still carries the `…ffffffff` synthetic tail, HV-13).
2. **A message queued before the link is up is lost.** `msg.transport_send_failed
   error="transport: not connected"` → `msg.all_sends_failed "requeuing"` →
   `msg.delivery_failed "delivery attempt budget exhausted"`. The engine retries
   for a bounded budget and drops the message rather than holding it until the
   link comes up. Iteration 1 of the smoke fails for exactly this reason.

**Part (2) fixed** (commit 28d0f1a): `TransportError::NotConnected` is now
`is_retryable()`, and `deliver_outbound` classifies a round whose only failures
were `NotConnected`/`Busy` as *transient* — `requeue_or_fail` then holds the
message (short 3 s retry, no `max_attempts` slot spent, TTL-bounded) instead of
failing it. The P2 reverse-ACK failures cleared too (`deliveryFailed` 9 → 0).
**Part (1)** — the ~30 s first-connect latency — is open as **HV-91**.

---

### HV-91 — BLE first-connect latency ~30 s (discovery / connect cadence)

- **Fix status:** 🟢 HW-verified · commit b4406be · 2026-09-01 · iris_bench
  tier-0 smoke on P1=V2205 + P2=vivo 2004: **iter-1 latency 27.7 s → 6.5 s
  cold / 0.45 s warm**, still 10/10 · Session 08. HV-80 (make scan/advertise
  modes adaptive once links are stable, to claw back the battery cost) is the
  remaining follow-up.
- **Resolution:** discovery loop scans at `active_scan_interval` (3 s) until a
  `connect()` actually succeeds (`has_confirmed_link`), then `scan_interval`
  (30 s); `ble.rs::discover_peers` keeps one scan running and only re-arms every
  ~5 passes (harvest-only passes no longer burn the `scan_allowed` throttle
  budget — the HV-14 conflation); scan `LOW_POWER → LOW_LATENCY`, advertise
  `LOW_POWER → BALANCED`.
- **Area:** `ble.rs` `scan_allowed` / `DiscoveryManager` scan cadence,
  `AndroidBleTransportAdapter.connectGatt` (`autoConnect=false`), HV-13 synthetic id
- **Severity:** High (UX — "the first message takes 30 s") · **HW gate:** 2 phones

**What:** `iris_bench` evidence (`session-02`): `startMesh` → first `GATT_Connect`
~33 s later, then connected in ~2 s. HV-90's hold means the first message is no
longer *lost*, but it still waits ~27 s. Candidates: the discovery scan-restart
cadence / backoff (HV-14), the first direct `connectGatt` being slow with
`autoConnect=false`, or the beacon-parse → connect ordering (the peer id still
carries the `…ffffffff` synthetic tail — HV-13). Measure with the harness;
target: first delivery under ~5 s.

### HV-92 — First send after connect fails with "characteristic not yet discovered" and the message is dropped, not held

- **Fix status:** 🟢 HW-verified · commit e6a4e72 · 2026-09-02 · P1=vivo V2205 (Android 15) P2=vivo 2004 (Android 13) · app-UI demo P1→P2 **15/15** (10/10 + 5/5 reconfirm), ~0.5 s/msg · Session 09 · with HV-93 (§5 group)
- **Area:** `AndroidBleTransportAdapter.onServicesDiscovered` (completes the
  `connectionReady` future even when the IRIS characteristic is absent from the
  freshly-discovered GATT DB), `crates/iris-core/src/transport/ble.rs`
  `to_transport_err` (`GattFailure` → `Protocol`, non-retryable),
  `crates/iris-core/src/message_engine/mod.rs` transient classification
- **Severity:** Critical (the first addressed message to a peer is silently lost) ·
  **HW gate:** 2 phones

**What:** observed on P1=V2205 → P2=vivo 2004 sending a real addressed message
through the app UI (`/addkey` + `/to` + `/send`). P1's discovery `connect()`
opened a GATT connection (handle 33) and `connectGatt` returned `Ok`; the first
`gatt_write` immediately failed:

```
msg.transport_send_failed transport=ble-android
  error="characteristic 3e5c6b1a2a104f6e9c315f3e5a0b0c0e not yet discovered"
msg.all_sends_failed message_id=01a05f839cf27a33 attempt=0
msg.delivery_failed  message_id=01a05f839cf27a33      ← dropped
```

~30 s later a fresh `connectGatt` completed service discovery cleanly
(`bta_gattc_explore_srvc_finished` / "ready resolved successfully") — but the
message was already gone.

**Mechanism:** Android's first `onServicesDiscovered` after a cold connect can
return an incomplete GATT database (well-documented OEM behaviour — Nordic /
Punch Through / Microchip BLE notes). `onServicesDiscovered` does
`gatt.getService(IRIS_SERVICE_UUID)?.getCharacteristic(...)?.let { cache }` then
**unconditionally** `connectionReady.complete(Unit)` — so when the service is not
yet in the DB, `serviceCharacteristics[gatt]` stays empty but Rust is told the
link is ready. The first `gatt_write` then throws `GattFailure("characteristic …
not yet discovered")`, which `to_transport_err` maps to `TransportError::Protocol`
— **non-retryable and non-transient** — so `message_engine` fails the message at
`attempt=0` instead of holding it (the HV-90 hold only covers `NotConnected` /
`Busy`).

**Fix direction:** (a) `onServicesDiscovered` must not report ready until the
IRIS characteristic is actually resolvable — retry `discoverServices()` a bounded
number of times, then fail the waiter exceptionally; (b) `to_transport_err` must
classify "characteristic not yet discovered" / "disconnected before connect
completed" / "write not initiated" as `NotConnected` (retryable + transient) so
the in-flight message is held across the reconnect, exactly as HV-90 intended.

**HW verification:** app-UI demo P1→P2 (`/addkey`, `/to`, `/send`) delivers the
message on P2's screen 10/10 with the phones cold; also the iris_bench tier-0
smoke stays 10/10.

### HV-93 — Inbound GATT-server frames are never drained: `onConnectionStateChange` doesn't fire for the accepting side, so no reassembly poller is spawned

- **Fix status:** 🟢 HW-verified · commit e6a4e72 · 2026-09-02 · P1=vivo V2205 (Android 15) P2=vivo 2004 (Android 13) · app-UI demo P1→P2 **15/15**; every `onCharacteristicWriteRequest` now followed by `msg.delivered hops=0` (was 0 deliveries) · Session 09 · with HV-92 (§5 group)
- **Area:** `AndroidBleTransportAdapter.gattServerCallback`
  (`onConnectionStateChange` / `onCharacteristicWriteRequest`), interacts with
  `crates/iris-core/src/transport/ble.rs::ensure_accept_poller`
- **Severity:** Critical (the peer receives every ATT frame and delivers none) ·
  **HW gate:** 2 phones

**What:** with HV-92's sender-side hold in place, P1's frames physically reach
P2 — P2 logs `onCharacteristicWriteRequest … matchesIris=true len=307` for every
send. But P2's `iris_core` never logs a single inbound-message event and the
message never appears in P2's console. `BluetoothGattServerCallback
.onConnectionStateChange` **never fired** for the inbound LE connection (its
`IrisBleDiag gattServer onConnectionStateChange` line is absent across the whole
session), so `pendingAcceptedConnections` stayed empty,
`ble.rs::ensure_accept_poller` never spawned a `spawn_inbound_poller` for the
peer, and the frames sat in `pendingGattWrites` undrained.

**Mechanism:** on several OEM stacks `onConnectionStateChange` on the *server*
callback is only invoked when the local side calls `BluetoothGattServer
.connect(device, false)` or the devices are bonded — a plain remote
`connectGatt` + characteristic write does not trigger it. AOSP's own
`BluetoothGattServer` docs note the server "will not be informed" of arbitrary
connections. IRIS cannot bond (privacy model: MAC ≠ identity), so it must
register the accepted connection from a signal that *does* arrive.

**Fix direction:** `onCharacteristicWriteRequest` carries the `BluetoothDevice`.
The first IRIS write from a device announces the accepted connection
(`pendingAcceptedConnections`, keyed by `deviceHash` — the same key the write
frames use), so the accept-poller spawns the reassembly poller and drains the
already-buffered frames. De-dup with a per-device `announcedInboundPeers` set,
cleared on server disconnect; `ble.rs` de-dups by handle so a redundant announce
is a no-op.

**HW verification:** app-UI demo P1→P2 (`/addkey`, `/to`, `/send`) — the message
renders on P2's console 10/10; iris_bench tier-0 smoke stays 10/10.

### HV-7 — Every real message is fragmented into many ATT writes, each a blocking round-trip, and one failure tears the whole link down

- **Fix status:** 🟢 HW-verified · commit 78743be · 2026-09-02 · P1=vivo V2205 (Android 15) P2=vivo 2004 (Android 13) · `iris_bench` `test_tier1_ble`: 300-char P1→P2 **10/10**, P2→P1 **10/10**, ping-pong **fwd 10/10 + rev 10/10 ×4 runs, 0 link teardown** (HEAD: 1 `msg.delivery_failed` + 34 s stall) · Session 10
- **Premise note:** the *fragmentation-storm* premise ("~12 serial writes per
  200-char message") is disproved on Android-14-class hardware — `requestMtu`
  reliably yields 517 so a 300-char message is ~3 frames. The *teardown-
  fragility* premise ("one failed write kills the link + drops the message; no
  retry") is confirmed — it just triggers on a spontaneous link blip under load,
  not fragment-7-of-12.
- **Resolution:** `ble.rs::send()` retries each ATT frame 3× (jittered
  250/500 ms) on a *retryable* error before `close_peer`; a write-completion
  timeout (`onCharacteristicWrite never fired` — the link dropped mid-write) is
  classified `NotConnected` so the message is **held** across the reconnect, not
  `DeliveryFailed`; an `unknown gatt connection` (phantom accept-poller route)
  drops that entry and returns `NotConnected` so the engine re-resolves to a
  real link. Sim: `SimulatedBleAdapter.fail_next_writes_transient` + 2 tests.
- **Owed:** HV-96 (the link drop itself) · HV-8 (reply-direction MTU on an
  asymmetric link).
- **Was:** ⬜
- **Area:** `crates/iris-core/src/transport/ble.rs` (`send`, `close_peer`,
  `AttSegmenter`), `crates/iris-core/src/transport/ble_att.rs`,
  `AndroidBleTransportAdapter.gattWrite`
- **Severity:** Critical · **HW gate:** 2 phones

**What:** `BleTransport` reports `max_message_size = BLE_MAX_MESSAGE_CONSERVATIVE`
(computed off the 23-byte default MTU). Any message longer than ~20 bytes of
payload — i.e. essentially every real message — is segmented into N ATT writes.
`AndroidBleTransportAdapter.gattWrite` (HW-7) now *blocks* on each write's
`onCharacteristicWrite` callback before returning, so the N fragments go out
strictly serially, each paying a full BLE round-trip (single- to low-double-digit
ms, worse under interference). Worse: `send()` calls `close_peer()` on the first
fragment write that fails — so a single dropped fragment on fragment 7 of 12
tears down the entire GATT connection and discards the partial message; the next
send has to re-discover and re-connect.

**Why it "sometimes works":** a 5-byte "hi" is one write and lands. A 200-char
message is ~12 serial writes; the probability that all 12 succeed under real RF is
noticeably below 1, and any miss is not retried at the fragment level — it kills
the link.

**Reasoning / chain:** MTU default 23 → payload/ATT ≈ 20 → fragment count high →
serial blocking writes → cumulative failure probability → no per-fragment retry →
`close_peer` on first failure → link churn → re-discovery latency → next send also
likely to straddle a scan-backoff window (HV-14). Every layer compounds.

**Fix sketch (needs research — see loop §2):**
1. **Negotiate a real MTU before the first send.** The central side calls
   `requestMtu(517)` in `onConnectionStateChange`/after service discovery;
   `connectGatt` should not resolve `connectionReady` until `onMtuChanged` has
   fired (or a short timeout). Today MTU is requested lazily via `setMtu` and the
   send path may run at 23 for the first message.
2. **Fragment-level retry** with bounded backoff instead of `close_peer` on the
   first miss. Only tear the link down after K consecutive fragment failures or a
   disconnect callback.
3. **Reconsider `BLE_MAX_MESSAGE_CONSERVATIVE`** for the manager's size filter —
   report the negotiated per-link cap once known so selection and the fragmenter
   agree.
4. **Accept-poller connections are stuck at `MTU_DEFAULT` forever** (see HV-8).

**HW verification:** send 10× a 300-char message each way between two phones; 10/10
delivered, no link teardown in logcat, median latency recorded. Repeat at 1 m,
5 m, and through one wall.

---

### HV-8 — Inbound (peripheral-role) connections never negotiate MTU — pinned at 23 bytes for the session

- **Fix status:** 🔒 Deferred (analysis) · 2026-09-02 · Session 10
- **Analysis:** the "pinned at 23" premise only reproduces on an **asymmetric**
  link. `ensure_accept_poller` registers the inbound connection with
  `connections.entry(peer_id).or_insert((handle, MTU_DEFAULT, 0))` — but with
  symmetric discovery (both phones dial each other, the common case) the central
  `connect()` inserts the real `(handle, 517, _)` first and the `.or_insert()`
  is a no-op, so the reply uses MTU 517. Bench: `test_p2_to_p1_300char_10x`
  (300-char reply) is **10/10** at HEAD without any HV-8 fix. The gap is real
  only when a node has an inbound link but no outbound one (connect backoff on
  one side / one-way beacon visibility) — and the Android peripheral role
  cannot write on a server connection anyway (the adapter's `gattWrite` needs a
  central `gattHandles` entry).
- **Proper fix (when scheduled):** add `onMtuChanged(device, mtu)` to
  `AndroidBleTransportAdapter.gattServerCallback`, carry it in
  `FfiAcceptedConnection` (bindings regen), and have `ensure_accept_poller`
  register the real value. Not on the Tier-1→Tier-2 gate critical path.
- **Was:** ⬜
- **Area:** `ble.rs` `ensure_accept_poller` (registers
  `(handle, MTU_DEFAULT, 0)`), `AndroidBleTransportAdapter` GATT server callbacks
- **Severity:** High · **HW gate:** 2 phones

**What:** HW-9 added the accept-poller so a connection a remote central dials to
*us* gets a reassembly poller. But it registers that connection in `connections`
with `MTU_DEFAULT` (23) and nothing ever updates it — the peripheral role in
Android never calls `requestMtu` (only the central does), and the GATT server
`onMtuChanged` is not wired to feed the Rust `connections` map. So when *this*
node is the peripheral and sends a reply on that same link, it fragments at 23
bytes even though the central negotiated 185+ on connect.

**Why it matters:** this is the "reply fails seconds after a successful receive"
symptom — the reply direction runs at the worst possible MTU and hits HV-7's
fragmentation fragility maximally.

**Fix sketch:** wire the GATT server's negotiated MTU
(`BluetoothGattServerCallback.onMtuChanged`, API 22+) into `pendingAcceptedConnections`
or a sibling drain, and have the accept-poller register the real value. Where the
platform does not surface it server-side, fall back to a conservative-but-not-23
value (e.g. 100) after the first successful inbound frame proves the link.

---

### HV-9 — The discovery beacon rides Manufacturer-Specific Data under the SIG test id `0xFFFF`

- **Fix status:** ⬜
- **Area:** `AndroidBleTransportAdapter.IRIS_MANUFACTURER_ID = 0xFFFF`,
  `startAdvertising`, `scanCallback.onScanResult`
- **Severity:** High (correctness under real ambient RF) · **HW gate:** 2 phones +
  a noisy RF environment

**What:** the 22-byte beacon is advertised as Manufacturer-Specific Data with
company id `0xFFFF` — the Bluetooth SIG's reserved "for testing" id. The scanner
reads `getManufacturerSpecificData(0xFFFF)` and feeds those bytes straight to
`DiscoveryBeacon::parse`. In any real environment, other apps and devices also use
`0xFFFF`; their payloads will be handed to `DiscoveryBeacon::parse`, which will
mostly fail `UnsupportedVersion` (fine) but is now attacker-reachable pre-auth and
wastes scan-result queue slots (bounded at `MAX_PENDING = 512`, oldest-drop — a
burst of foreign `0xFFFF` ads can evict real IRIS beacons before the Rust side
drains them).

**Why it matters:** intermittent discovery failures in a crowded space
("sometimes finds the peer") are consistent with the real beacon being evicted
from a 512-deep oldest-drop queue by ambient `0xFFFF` traffic.

**Fix sketch (research):** (a) short term — add a 2–4 byte IRIS magic prefix
*inside* the manufacturer data and reject non-matching payloads before they reach
the queue or `parse`; (b) medium term — decide whether to register a real company
id or move to a 16-bit service UUID in Service Data (fits if the beacon is
trimmed, or use BLE 5 extended advertising where available with a legacy
fallback — one of the two bench phones lacks extended advertising, per the code
comment). Document the collision-safety analysis.

---

### HV-10 — Advertising failure is logged and abandoned — no retry, no state signal

- **Fix status:** ✅ Fixed · HW-PENDING · commit 0875136 · 2026-09-03 · Session
  14 · §5 group with HV-31 · no bench trigger for `TOO_MANY_ADVERTISERS`
  (needs many rapid advertiser start/stop cycles) — retry logic is
  code-complete + L1-tested only
- **Progress:** `launchAdvertising()` extracted; `onStartFailure` schedules a
  bounded exponential-backoff retry (1 s / 4 s / 10 s) for the transient codes
  (`TOO_MANY_ADVERTISERS`, `ALREADY_STARTED`, `INTERNAL_ERROR`), always
  `stopAdvertising` first; `DATA_TOO_LARGE` / `FEATURE_UNSUPPORTED` terminal.
  After exhaustion → `drainAdapterEvents()` code `1`, which makes the core
  re-drive `start_advertising` from its cached `NodeAdvertisement`. No hardware
  trigger for `TOO_MANY_ADVERTISERS` yet — logic is code-review + negative
  assertion only. See `hardware_fix_log.md` Session 14.
- **Area:** `AndroidBleTransportAdapter.startAdvertising`
  (`AdvertiseCallback.onStartFailure`)
- **Severity:** High · **HW gate:** 2 phones (force failure by over-sizing the ad
  or starting many advertisers)

**What:** `onStartFailure(errorCode)` does `Log.w(...)` and
`advertiseHandles.remove(handle)`. That's it. The Rust `BleTransport` already
returned `Ok` from `start_advertising` (the failure is async). So a device whose
advertising failed — `ADVERTISE_FAILED_TOO_MANY_ADVERTISERS` (common after
rapid start/stop cycles), `DATA_TOO_LARGE`, `INTERNAL_ERROR` — is **invisible to
peers for the rest of the session** while reporting itself as advertising, and
nothing retries.

**Why it matters:** "the other phone stopped seeing me" with no error. Classic
after a few reconnect cycles that each restart the advertiser.

**Fix sketch:** on `onStartFailure`, schedule a bounded exponential-backoff retry
of `startAdvertising`; surface a degraded state to Rust (a new drain the transport
polls, or fold into `is_available`); count failures in metrics (HV-6). For
`TOO_MANY_ADVERTISERS`, ensure `stopAdvertising` is always called before a
restart (audit every path).

---

### HV-11 — `onScanFailed` is logged only; the core's `scan_allowed()` backoff never learns the platform refused

- **Fix status:** ✅ Fixed · HW-PENDING · 2026-09-02 · Session 11 · §5 group with HV-14/15
- **Resolution:** new `BleAdapter::drain_scan_failures() -> Vec<i32>` (+ the
  `FfiBleAdapter` foreign-trait method, bindings regenerated). Kotlin
  `onScanFailed` enqueues the code; `discover_peers` drains it each pass and on
  a hard code (2 `APPLICATION_REGISTRATION_FAILED`, 3 `INTERNAL_ERROR`,
  6 `SCANNING_TOO_FREQUENTLY`) clears `scan_handle` (forces a real re-arm) and
  on 6 arms `refused_until`. `tracing::warn!(event = "ble.scan_refused")`.
  L1 sim `hv11_platform_scan_refusal_forces_a_rearm_and_arms_backoff`.
- **HW-pending:** no L3 trigger for `SCANNING_TOO_FREQUENTLY` without a test
  hook; the 3× 40-round sustained sessions logged zero `ble.scan_refused`.
- **Was:** ⬜
- **Area:** `AndroidBleTransportAdapter.scanCallback.onScanFailed`,
  `ble.rs` `scan_allowed`, `discover_peers`
- **Severity:** High · **HW gate:** 2 phones

**What:** HW-1 established that `startScan` called twice without a `stopScan`
returns normally then fires `onScanFailed(SCAN_FAILED_ALREADY_STARTED)` async.
`discover_peers` now stops the prior scan first — good. But `onScanFailed` still
only does `Log.w`. Any *other* async scan failure —
`SCAN_FAILED_APPLICATION_REGISTRATION_FAILED`, `SCAN_FAILED_INTERNAL_ERROR`,
`SCAN_FAILED_SCANNING_TOO_FREQUENTLY` (Android 7+ 5-scans-per-30s hard limit,
distinct from IRIS's own ceiling) — leaves the Rust side believing a scan is
live (`scan_handle` is `Some`) when the OS has refused it. `scan_allowed()`'s
window/backoff is driven by IRIS's own call cadence, not by the platform's actual
verdict.

**Why it matters:** the IRIS ceiling (5/30s) and the Android ceiling (5/30s) are
the same number but count different events (IRIS counts `discover_peers` calls;
Android counts `startScan` syscalls including internal restarts). They drift, and
when Android refuses, IRIS keeps thinking it is scanning.

**Fix sketch:** route `onScanFailed` into a drain the transport polls (mirror the
`accepted_connections` pattern); on `SCANNING_TOO_FREQUENTLY` / `ALREADY_STARTED`,
clear `scan_handle` and arm `refused_until` in `scan_allowed()` directly. Add a
metric.

---

### HV-12 — RSSI floor is inconsistent across three layers (-85 / -95 / -127)

- **Fix status:** 🟢 HW-VERIFIED · commit 49413e5 · 2026-09-04 · Session 20 · `RSSI_FLOOR_DBM` -85 → **-95** (single source; the Android
  adapter's `rssiFloor` default -127 → -95 to match; the bridge default was
  already -95). Session-20 bench (`session-20-tier1`): `test_pingpong_300char_10x`
  fwd 10/10 + rev 10/10, 0 churn; `test_sustained_session_recovers_fast` 40/40
  both ways, 0 recovery gaps — discovery + delivery reliable with the unified
  -95 floor across a full session. -95 is strictly more permissive than the old
  -85 so it cannot reduce discovery. L1 `hv12_rssi_floor_is_one_mesh_appropriate_value`.
  **Follow-up (L4 manual, non-blocking):** the effective-range map — at what
  distance / wall count does discovery actually stop.
- **Was:** ⬜
- **Area:** `ble.rs::RSSI_FLOOR_DBM = -85`, its doc comment says "core filter
  default -95", `AndroidBleTransportAdapter.rssiFloor` defaults to `-127` and is
  only set from `filter.rssiFloor` when a scan actually starts
- **Severity:** Medium · **HW gate:** 2 phones at varying distance

**What:** three different floors. The Rust constant is -85 dBm (≈10 m open
office). Its own doc says the default is -95. The Kotlin adapter starts at -127
(accept everything) and adopts the filter value only inside a successful
`startScan` — a refused start leaves it at whatever it was. -85 dBm is
aggressive: a phone in a pocket or one wall away is routinely -88 to -95 and would
be filtered out entirely.

**Why it matters:** "it only works when the phones are right next to each other" is
exactly a -85 dBm floor. And the floor that actually applies depends on which
layer won.

**Fix sketch:** one constant, threaded from core config to the adapter, defaulting
to about -95 dBm for a mesh (tunable via `/diag`). Verify the effective range on
the bench: at what distance / wall count does discovery stop.

---

### HV-13 — `known_addresses` (MAC → candidate PeerId) is only populated by `discover_peers`, and never pruned

- **Fix status:** ✅ Fixed (part b) · HW-PENDING · commit 49413e5 · 2026-09-03 · Session 15 · `known_addresses` value → `(PeerId, Instant)`; `discover_peers`
  sweeps entries older than `KNOWN_ADDR_TTL` (300 s) + caps at `KNOWN_ADDR_CAP`
  (128, oldest-first) each pass; `close_peer` evicts the dropped peer's MAC
  hints. L1 `hv13_known_addresses_is_bounded_and_evicted_on_close`; forced-drop
  regression exercises the `close_peer` eviction. **Part (a) deferred** — the
  split synthetic/real peer-id reconciliation is an envelope↔transport
  cross-layer change and genuinely needs 2–3 phones + a crafted
  advertise-before-scan race.
- **Was:** ⬜
- **Area:** `ble.rs` `known_addresses`, `discover_peers`, `ensure_accept_poller`,
  `close_peer`
- **Severity:** Medium · **HW gate:** 2–3 phones

**What:** the accept-poller resolves an inbound connection's peer identity via
`known_addresses.get(&address)`, falling back to `synthesize_unknown_peer_id`
(a placeholder hash). `known_addresses` is written only when `discover_peers`
successfully parses a beacon from that MAC. So if phone B dials phone A *before* A
has completed a discovery pass that saw B's beacon (entirely normal — advertising
and scanning are independent loops on independent cadences), A attributes B's
inbound frames to a synthetic peer id. The envelope layer re-attributes the real
sender on decrypt, so delivery still works — but any transport-level state keyed
by peer id (the `connections` entry, the poller registration, `close_peer`
targeting) is now split across two ids for one physical device, mirroring the
Wi-Fi Direct FFI-1/FFI-18 bug class. The map also grows unbounded across a
long-lived node's peer churn.

**Fix sketch:** (a) when the envelope layer resolves a real sender for a frame
that arrived on a synthetic-id connection, reconcile the `connections`/`pollers`
entries to the real id (one migration point); (b) bound `known_addresses` with an
LRU and evict on `close_peer` + a TTL; (c) consider persisting recently-seen
MAC→candidate hints across a short app restart so a reconnect after a crash does
not start blind.

---

### HV-14 — After 5 scan restarts in 30 s the node stops scanning for up to 30 s — and reconnect cycles burn restarts

- **Fix status:** 🟢 HW-verified · commit 366f080 · 2026-09-02 · P1=vivo V2205 (Android 15) P2=vivo 2004 (Android 13) · `iris_bench` `test_sustained_session_recovers_fast` ×3 (40-round bidirectional): **fwd 40/40, rev 40/40, 0 loss each** · Session 11 · §5 group
- **Resolution:** HV-91 already de-conflated the `scan_allowed` budget
  (harvest-only passes don't re-arm). The residual bug: `has_confirmed_link`
  (which picks the fast 3 s vs slow 30 s scan cadence) was set `false` **only**
  by the 300 s neighbor-TTL sweep, so a `close_peer` link drop left the scan
  slow for up to 30 s. `DiscoveryManager::scan_once` now recomputes
  `has_confirmed_link` from **actual transport state** (`any transport.state()
  == Connected`) every pass — a drop resumes fast scanning on the next pass.
- **Verified:** 3× clean 40-round bidirectional sessions (240 messages, 0 loss)
  + the HV-7 ping-pong regression, all pass with the change in.
- **Was:** ⬜
- **Area:** `ble.rs` `scan_allowed` (SCAN_CEILING=5, exponential backoff to 30 s),
  `ScanRestartPolicy`, `discover_peers`
- **Severity:** High · **HW gate:** 2 phones, force reconnects

**What:** `scan_allowed()` refuses the 6th scan start in a 30 s window and arms an
exponential backoff (1→2→4…→30 s). Every `discover_peers` call is one restart.
A link drop (HV-7's `close_peer`, a real disconnect, a Wi-Fi Direct channel
hiccup) triggers re-discovery, which is another restart. Under the "link carries
one message then drops" symptom, the node can easily hit 5 restarts in 30 s and
then **go blind for 30 s** — during which the peer is undiscoverable and
unreconnectable. This is a plausible mechanism for "works, then dead for ~30 s,
then works again."

**Why it matters:** the throttle exists to respect the Android 7+ hard limit, but
its interaction with reconnect churn creates a self-reinforcing outage: drop →
re-discover → hit ceiling → 30 s blind → drop persists → repeat.

**Fix sketch (research):** (a) separate "scan to widen the mesh" (throttleable)
from "scan to reconnect a known peer that just dropped" (should use a targeted
`ScanFilter` by address, which some stacks count differently / more leniently);
(b) hold established GATT connections through a discovery-blind window — do not
require re-discovery to reconnect a peer whose MAC is known; (c) if the ceiling is
hit, keep *existing* links alive rather than tearing everything down; (d) tune the
ceiling/window against the actual Android version behaviour on the bench phones.

---

### HV-15 — No automatic reconnect: a dropped GATT link is only re-established by the next discovery pass finding the beacon again

- **Fix status:** 🟢 HW-VERIFIED · commit 553fa4d (+ HV-105/107/109) · 2026-09-04 · Session 20 · targeted reconnect from the cached address + `DiscoveryManager::wake()`. **Session-20 `session-20-hv109b`: `test_forced_drop_reconnect_10x` 10/10, recovery 3.3–10.5 s, 0 `msg.delivery_failed`.** End-to-end message recovery after a forced drop had regressed to a permanent stall through Sessions 15–19 (root causes **HV-105** + **HV-107** + **HV-109**, all fixed this session); the 10/10 run restores 🟢.
- **Premise note:** partly disproved — `scan_transport` already calls `connect()`
  (idempotent) for every peer *harvested this pass*, so a drop heals without a
  full handshake **if** the beacon is in the current scan buffer. The gap: a
  dropped link's beacon can be absent from the buffer for several passes.
- **Resolution:** `DiscoveryManager` caches every `PeerInfo` it has connected
  (`known_peers`, bounded `2×max_peers`); the per-beacon connect block is
  extracted to `attempt_connect()`; `scan_once` also runs it — while there is no
  live link — for every known peer still in the neighbor table whose beacon was
  not harvested this pass. `connect()` needs only the cached address, no scan.
- **HW-pending:** no GATT link drop occurred in the 3× sustained bench sessions
  (the post-HV-7 desired outcome), so the reconnect firing was not observed.
  Closing to 🟢 needs a `dropLink()` snippet RPC (HV-97).
- **Was:** ⬜
- **Area:** `ble.rs` `close_peer`, `DiscoveryManager`, `connect`
- **Severity:** High · **HW gate:** 2 phones

**What:** when `close_peer` runs (write failure, disconnect), the connection entry
and poller are removed. Nothing schedules a reconnect. The link comes back only
when `DiscoveryManager`'s scan loop next parses that peer's beacon and calls
`connect()` again — which depends on the beacon being in the scan window, the scan
not being backed off (HV-14), and the beacon still advertising (HV-10). Android's
`BluetoothDevice.connectGatt(autoConnect = true)` exists precisely for "reconnect
when the device is back in range" and is not used (`connectGatt` passes
`false`).

**Fix sketch:** on `close_peer` for a peer whose MAC is known, either (a) issue a
direct `connectGatt` retry with bounded backoff (targeted, does not consume a scan
restart), or (b) register an `autoConnect = true` GATT connection so the platform
handles re-establishment. Keep the poller registration warm across a short drop
instead of destroying it.

---

### HV-16 — GATT server / characteristic UUIDs: verify Android and the core actually agree on the wire

- **Fix status:** 🟢 HW-VERIFIED · commit 8c0a986 · 2026-09-04 · Session 20 · regression guard landed + transitively HW-verified: Session-20
  `test_pingpong_300char_10x` connected P1↔P2 GATT and exchanged 300-char frames
  10/10 both ways with 0 churn — the Android GATT server's service UUID, the
  scanner filter UUID, and every FFI-threaded characteristic UUID are mutually
  consistent on real hardware (a mismatch fails every write, per BLE-3). The
  four UUID sites agree at HEAD (Kotlin
  `IRIS_SERVICE_UUID` was unified to `01000000-…` by CROSS-001 `9bddad9`; the
  write path already threads `IRIS_WRITE_CHARACTERISTIC` from the core across
  the FFI). Two coupled tests now fail the build on any drift:
  `iris-android/src/bridge.rs::hv16_ble_uuids_match_the_canonical_wire_values`
  (core `pub const`s → canonical hex, all distinct) and
  `android/.../BleUuidParityTest.kt` (parses those literals, asserts the Kotlin
  constants match + non-collision). UUID agreement is transitively HW-verified
  by every passing BLE delivery test this pass (HV-7/92/93/98 all exercise the
  write characteristic + GATT-server match). iOS `IrisBleConstants` flagged in
  both tests' comments (not on the bench).
- **Was:** ⬜
- **Area:** `AndroidBleTransportAdapter.IRIS_SERVICE_UUID` /
  `IRIS_CHARACTERISTIC_UUID` vs `ble.rs::IRIS_WRITE_CHARACTERISTIC` /
  `IRIS_SERVICE_UUID` / `IRIS_IDENTIFY_CHARACTERISTIC`
- **Severity:** High (regression guard) · **HW gate:** 2 phones + `adb`

**What:** the core has three UUIDs (`IRIS_SERVICE_UUID = [1,0,0,...]`,
`IRIS_IDENTIFY_CHARACTERISTIC = [2,0,0,...]`, `IRIS_WRITE_CHARACTERISTIC =
3e5c6b1a-...-0c0e`). The Android adapter defines
`IRIS_SERVICE_UUID = 3e5c6b1a-...-0c0d` and `IRIS_CHARACTERISTIC_UUID =
3e5c6b1a-...-0c0e`. The core's `IRIS_SERVICE_UUID` (`[1,0,0,...]` → 
`01000000-0000-0000-0000-000000000000`) is **not** the Android adapter's service
UUID. The write characteristic matches (`...0c0e`). BLE-3's comment says `send()`
used to pass the wrong UUID and every write failed. This needs an explicit,
tested cross-check that: the service the Android GATT *server* registers, the
service UUID the *scanner* filters on, the service UUID in the *beacon /
manufacturer data*, and every UUID the core hands across FFI are mutually
consistent — and a test that fails loudly if any drifts.

**Fix sketch:** a single shared constants source (generate the Kotlin constants
from the Rust ones, or a conformance test that asserts equality across the FFI
boundary). Confirm on `adb` (`dumpsys bluetooth_manager` / a GATT explorer app)
that the advertised service and the server's service match.

---

### HV-17 — `deviceHash` (SHA-256 of MAC) vs random/resolvable-private BLE addresses

- **Fix status:** ⬜
- **Area:** `AndroidBleTransportAdapter.deviceHash`, `toBluetoothMac`,
  `connectGatt`
- **Severity:** Medium · **HW gate:** 2 phones, one advertising with RPA

**What:** the FFI `handle` for a server-side write is
`SHA-256(mac)[..8]`. Android peripherals frequently advertise with a
**Resolvable Private Address** that rotates (~15 min) unless both devices are
bonded. If phone B's RPA rotates mid-session, phone A's scanner sees a "new"
device, `deviceHash` changes, and the accept-poller treats continued traffic from
B as a new peer — while the old `connections` entry and poller for B's previous
address leak. `connect_gatt` also takes a bare 12-hex MAC from the bridge; a
rotating address means the address you discovered may not be connectable minutes
later.

**Fix sketch (research):** determine whether IRIS should require BLE bonding (adds
a pairing UX but stabilises the address and enables `autoConnect`), or whether
identity must be fully decoupled from the MAC at the transport layer (it partly is
— candidate ids — but `connections`/`known_addresses`/`deviceHash` are all
MAC-keyed). Document the address-privacy interaction with `PRIVACY_MODEL.md`.

---

### HV-18 — Foreground-service + a user's Bluetooth headset/watch: BLE radio contention untested

- **Fix status:** ⬜
- **Area:** `IrisBleService`, adapter scan/advertise/GATT, Android BLE stack limits
- **Severity:** Medium · **HW gate:** 2 phones + a paired BT audio device / watch

**What:** the operator raised: "if the user is connected to their smart watch over
Bluetooth, then what?" Android's BLE stack has finite concurrent-GATT and
advertising-set budgets shared across the whole system. A phone with an active
A2DP headset + a BLE watch + IRIS scanning + IRIS advertising + IRIS GATT server +
N IRIS client connections can hit `TOO_MANY_ADVERTISERS`, GATT `133` errors, or
scan starvation. None of this is tested or handled.

**Fix sketch:** bench with a realistic loadout (headset connected, watch paired),
measure IRIS discovery/delivery success, and add graceful degradation: cap
concurrent IRIS GATT connections, back off advertising when the system is
contended, surface a "radio contended" diagnostic.

---

### HV-94 — Sender keeps a zombie GATT link after the peer's app process restarts: writes "succeed" into a dead server, no liveness check, no reconnect

- **Fix status:** 🟢 HW-verified · commit 28272c0 · 2026-09-03 · P1=vivo V2205 (Android 15) P2=vivo 2004 (Android 13) · Session 15 · `iris_bench`
  `test_peer_process_restart_recovers` (`am force-stop` P2 ×3): **3/3**, P1
  recovery [0.9, 0.4, 0.5] s, **0 `msg.delivery_failed`**, 0 "unknown gatt
  connection" fallbacks.
- **Fix:** the client-side `BluetoothGattCallback.onConnectionStateChange`
  DISCONNECTED now enqueues the dropped handle to `pendingDisconnectedHandles`,
  exposed as `drainDisconnectedHandles(): List<Long>` — the mirror of HW-9's
  server-side `acceptedConnections()`. `FfiBleAdapter::drain_disconnected_handles`
  + `BleBridge` map it to `GattHandle`; the accept-poller's existing
  `drain_disconnected_handles` consumer (BLE-1, previously fed only by the
  simulator) tears the peer's `connections` entry + poller down. So a peer whose
  app process dies is noticed immediately, not on the next failed `send()`.
  L1 test `hv94_client_disconnect_tears_down_peer_before_next_send`.
- **Note:** the "tens of seconds to never" premise was already disproved on HEAD
  by the accumulated HV-7/HV-15/HV-98/HV-99 self-healing (verified 3/3 before
  this fix); the fix removes the one-message-lost + the late `close_peer`.
- **Area:** `ble.rs` `send` / `close_peer` / connection liveness, interacts with
  HV-15 (dropped-link reconnect) and HV-27 (per-link health)
- **Severity:** High · **HW gate:** 2 phones

**What:** distinct from HV-15, where both sides see the link drop. Here the
Android ACL/GATT connection **stays up from the central's (P1's) perspective**
while the *peripheral's app process* (P2) is killed and relaunched underneath it.
P1's `connections` entry and poller are untouched, `gatt_write` keeps returning
`Ok` (the frames leave P1's radio), so nothing triggers `close_peer` — but P2's
new process has a fresh GATT server, a fresh engine, and fresh X25519 keys, so
the frames are either not routed to the new server object or cannot be decrypted.
Observed: after `am force-stop` + relaunch on P2, P1's subsequent `/send`s log
`msg.sent` normally and P2 shows nothing until the discovery loop eventually
re-churns the peer (tens of seconds to never).

**Why it matters:** an OOM-kill or a user swipe-away of the IRIS app on one phone
silently blackholes every message the other phone sends until a full
rediscovery. The operator will read it as "it just stopped working."

**Fix sketch:** (a) a cheap liveness signal on the central side — treat a run of
writes with no corresponding ACK / no `onCharacteristicWrite` confirmation, or a
GATT error, as "peer gone" and `close_peer` + reconnect; (b) or an application
heartbeat on the IRIS characteristic; (c) the peripheral, on startup, should
proactively tear down any lingering server-side connections so the central sees a
real disconnect. Coordinate with HV-15's reconnect path and HV-27's per-link
health.

---

### HV-95 — `openGattServer` returning null is swallowed — the node advertises an IRIS beacon with no GATT server behind it

- **Fix status:** ✅ Fixed · HW-PENDING · commit 5939a91 · 2026-09-03 · Session 16 · `ensureGattServer(): Boolean` (bounded 3× retry);
  `startAdvertising` throws `Transport("BLE GATT server unavailable…")` when it
  fails so the core records `ble-android` as not-started (not half-up); core
  watchdog on the 5 s `poll_health` tick re-drives `start_advertising` whenever
  `adv_handle` is None but a beacon is cached (`ble.advertise_watchdog`), so a
  self-healing wedge recovers in ~5 s with no BT toggle. L1
  `hv95_advertise_watchdog_redrives_a_failed_start`. Bench: no wedge trigger on
  a healthy Funtouch stack — `test_p1_to_p2_300char_10x` **10/10**, 0 spurious
  watchdog fires. The wedge-recovery path itself needs a stack that actually
  wedges (Session 09 saw it after heavy churn + reinstalls).
- **Was:** ⬜ (found in Session 09; recovered manually with a Bluetooth off/on toggle)
- **Area:** `AndroidBleTransportAdapter.ensureGattServer` / `startAdvertising`,
  `ble.rs` `handle_adapter_events` watchdog
- **Severity:** High · **HW gate:** 2 phones

**What:** `ensureGattServer()` calls `bleManager.openGattServer(...)`; when the
Bluetooth stack is wedged (seen after heavy connect/disconnect churn + repeated
reinstalls — `openGattServer` returns `null`, `startScan` fails
`errorCode=2 SCAN_FAILED_APPLICATION_REGISTRATION_FAILED`, `dumpsys
bluetooth_manager` stops reporting adapter state) it logs
`ensureGattServer: openGattServer returned null` **and returns** — but
`startAdvertising` then proceeds anyway. The node broadcasts a connectable IRIS
beacon it cannot serve: every peer that connects fails
"characteristic not yet discovered" forever (the exact HV-92 symptom, but here
the cause is a missing server, not an incomplete discovery). Only a manual
`svc bluetooth disable/enable` (or reboot) recovered it in Session 09.

**Fix sketch:** if `openGattServer` returns null (or `addService` returns false),
`startAdvertising` must fail with a typed error so the core's transport-start
path records `ble-android` as failed rather than half-up; retry `openGattServer`
with bounded backoff; surface "BLE server unavailable — toggle Bluetooth" in
`/diag`. Consider a watchdog that re-runs `ensureGattServer` when the server
handle is null while advertising is supposedly active.

---

### HV-96 — A BLE GATT link drops spontaneously under sustained bidirectional load

- **Fix status:** 🔬 (found in Session 10 verifying HV-7's ping-pong)
- **Area:** `AndroidBleTransportAdapter` (connection priority / parameters),
  `ble.rs`; interacts with HV-91 (reconnect latency), HV-15 (auto-reconnect)
- **Severity:** High · **HW gate:** 2 phones **+ `btsnoop` HCI snoop log**

**What:** with HV-7's per-frame retry + hold in place, a 300-char ping-pong
(receive-then-reply, 10 rounds) still occasionally drops the GATT link mid-run:
`gattServer onConnectionStateChange … status=0 newState=0` fires on **both**
phones within ~3 s, preceded by repeated `onConnectionUpdated interval=6 …
interval=36` (connection-parameter renegotiation). HV-7's fix makes the
in-flight message *held* (recovers in ~0.5 s instead of a `msg.delivery_failed`
+ ~30 s cold reconnect), so the ping-pong now passes — but the underlying drop
is a real defect and will dominate the loop §2 "30-minute session, zero
unexplained link losses" gate.

**Why `status=0`:** a clean/local disconnect, not a supervision timeout
(`status=8`). One side is tearing the link down — candidate causes: IRIS's own
`close_peer` on a transient error (now retried, so less likely), the OS
dropping the link under write pressure, or a connection-priority/parameter
conflict. Needs the HCI log to see the LL `LL_TERMINATE_IND` reason code.

**Fix sketch (pending research):** `requestConnectionPriority(CONNECTION_
PRIORITY_HIGH)` after connect for the active-messaging window; do not let both
sides renegotiate parameters simultaneously; confirm `close_peer` is not
firing; keep the poller warm across a <2 s drop (HV-15).

**HW verification:** the 30-minute two-phone bidirectional session (loop §2
Tier-1→Tier-2 gate) shows zero unexplained link losses, with `btsnoop`
confirming any that do occur are external RF, not a self-inflicted teardown.

### HV-97 — `iris_bench`: back-to-back tests fail; evidence dir bumps per process

- **Fix status:** ✅ Fixed · 2026-09-02 · Session 12 · plus a new `dropAllLinks()` hook
- **Resolution:** `_make_session_dir` keyed off `$IRIS_BENCH_RUN` (or a
  timestamp), not a folder scan. `AndroidBleTransportAdapter.stopAdvertising`
  now `gattServer?.close()` + nulls it + clears `announcedInboundPeers` when the
  last advertiser stops (the server was opened lazily and never closed → a
  `stopMesh` → new adapter → `startMesh` cycle left a second stale
  `BluetoothGattServer`). Also added `Transport::drop_all_links()` +
  `IrisEngine::drop_all_links()` FFI + `IrisSnippet.dropAllLinks()` RPC +
  `DiscoveryManager::wake()` — the deterministic reconnect trigger the harness
  needs (used to verify HV-14/HV-15/HV-98). Sim
  `hv97_drop_all_links_clears_connections_but_stays_reconnectable`.
- **Owed:** full re-verify of "two test methods in one process" once HV-98
  (inbound poller re-attach) lands — the stale GATT server was one cause, the
  inbound-poller lifecycle is the other.
- **Was:** 🔬 (found in Session 10)
- **Area:** `iris_bench/base.py` (`_make_session_dir`, `stop_mesh_both` /
  `start_mesh_both`), `IrisSnippet.stopMesh`/`startMesh`
- **Severity:** Medium (test-integrity) · **HW gate:** 2 phones

**What:** running two test methods in one `python -m iris_bench` invocation:
the first passes, the second (opposite direction) times out for 5×90 s. Each
`setup_test` does `stopMesh` then `startMesh`, which builds a fresh
`IrisEngine` over a fresh `AndroidBleTransportAdapter` — but the *previous*
adapter's `BluetoothGattServer` and scan/advertise callbacks are not fully torn
down, so the OS BLE stack is left with duplicate registrations (HV-95 family).
Also `_make_session_dir` increments `session-NN` on every `setup_class` (every
process), littering `evidence/` with `session-11…session-20`.

**Fix sketch:** `IrisEngine.stopAll` / the adapter must `close()` the GATT
server and unregister every callback; `iris_bench` should reuse one engine per
class where possible, or force a BT cycle between tests; `_make_session_dir`
keyed off a run id / env var, not an incrementing scan of the folder.

---

### HV-98 — Inbound reassembly poller does not re-attach after a drop + reconnect

- **Fix status:** 🟢 HW-verified · commit `81e17cd` · 2026-09-03 · P1=vivo V2205
  (Android 15) P2=vivo 2004 (Android 13) · `iris_bench`
  `test_forced_drop_reconnect_10x` **10/10**, recovery ~3.5 s each, 0
  `msg.delivery_failed`
- **Fix (Session 13):** `onCharacteristicWriteRequest` announces the accepted
  connection on **every** IRIS write (dropped the fire-once `Set` and the
  time-gap heuristic). Dedup + re-attach is now owned by the Rust accept-poller:
  a new shared `BleTransport.accept_spawned: Arc<Mutex<HashSet<GattHandle>>>`
  (was a task-local set, unreachable from `close_peer`); `close_peer` /
  `shutdown` clear the handle, so the first write after our own `drop_all_links`
  re-spawns the inbound reassembly poller while a still-live handle stays a
  cheap no-op. L1 sim regression: `hv98_inbound_poller_re_attaches_after_our_own_drop`.
- **Area:** `AndroidBleTransportAdapter` (`announcedInboundPeers` lifecycle,
  `gattServerCallback`), `ble.rs` `ensure_accept_poller` / `close_peer` vs the
  accept-poller's `spawned` set
- **Severity:** Critical (a peer that re-links after any drop stops receiving) ·
  **HW gate:** 2 phones

**What:** with HV-97's `dropAllLinks()` hook, P1 and P2 both re-establish the
GATT link in ~1 s and P1's ATT frames reach P2's characteristic
(`onCharacteristicWriteRequest matchesIris=true len=300`) — but P2 emits no
`msg.delivered`. P2's GATT *server* never fires
`onConnectionStateChange(DISCONNECTED)` for P1's connection (OEM: server-side
disconnect callback is unreliable — HV-93/HV-94), so `announcedInboundPeers`
keeps the stale `deviceHash`, `onCharacteristicWriteRequest` does not
re-announce the accepted connection, and `ensure_accept_poller` (whose
`spawned` set also still holds the handle) never re-spawns the inbound
reassembly poller. P2's own `drop_all_links` → `close_peer` had aborted the
poller that was there.

**Fix sketch:** (a) `onCharacteristicWriteRequest` re-announces whenever there
is no *live* inbound poller for the device (track liveness, not a fire-once
`Set`); (b) or the accept-poller treats "frames arriving on a handle with no
poller" as a re-attach trigger; (c) clear `announcedInboundPeers` / the
accept-poller's `spawned` on our own `close_peer` / `drop_all_links`, not only
on the OEM server-disconnect callback. Coordinate with HV-93/HV-94.

---

### HV-99 — `connectGatt` reconnect probe blocks the full 30 s FFI budget when the peer is not advertising

- **Fix status:** 🟢 HW-VERIFIED · commit a645b90 (+ HV-107/109) · 2026-09-04 ·
  Session 20 · `connectGatt` 12 s probe ceiling (was Android's fixed ~30 s);
  `CONNECT_BACKOFF_BASE` 5 s (was 30 s); BLE `drain_adapter_events` code 2 tears
  down GATT links; `Transport::poll_health` + a 5 s discovery health tick.
  **Session-20 `session-20-hv109b`: `test_bluetooth_toggle_recovers` 3/3, all
  53.9–57.0 s (< 60 s budget)** — the P1-side reconnect-cadence + peripheral
  link-death gap that held this at 2/3 was closed by HV-107 (outbound dial no
  longer blocked) + HV-109 (persistent GATT server). A tighter < 30 s target is
  a nice-to-have, not a blocker.
- **Area:** `AndroidBleTransportAdapter.connectGatt` (`FfiCallTimeout` budget /
  `connectionReady` wait), `ble.rs` `connect` / discovery retry cadence —
  interacts with HV-15, HV-31, HV-94
- **Severity:** High · **HW gate:** 2 phones

**What:** on P1, a reconnect attempt to a peer that is temporarily not
advertising (peer rebooting Bluetooth, peer app restarting, peer briefly out of
range) calls `connectGatt`, which blocks the full 30 s `FfiCallTimeout` budget
before returning `Timeout`. The discovery loop cannot retry, switch transports,
or re-scan for those 30 s — per attempt. Observed (`session-14-hv31c`):
`iris.ble.gatt connectGatt: initiated` → 30 002 ms → `discovery.connect_failed
… elapsed_ms=30002`, immediately retried, another 30 s. A discovery-driven
reconnect probe should fail in ~5 s so the next 3 s `active_scan_interval` pass
picks it up. Blocks HV-31, HV-94, and the §2 30-minute-session gate.

**Fix sketch:** a short per-attempt connect timeout for the *discovery-driven*
reconnect path (distinct from a user-initiated `send`, which may wait longer);
or `connectGatt` with `autoConnect=false` + a 5–8 s ceiling, letting the
discovery loop own the retry cadence. Also plumb `DiscoveryManager::wake()` to
`Transport`-surfaced events (BLE `drain_adapter_events`) so an adapter recovery
is felt in ~1 s, not up to `scan_interval`.

---

### HV-105 — `close_peer` evicting the MAC→candidate hint permanently breaks every teardown-then-reconnect path

- **Fix status:** 🟢 HW-VERIFIED · commit `618e673` · 2026-09-04 · Session 20 ·
  reverted the `known_addresses` eviction that commit `49413e5` (HV-13) added to
  `close_peer`. L1 `hv13_known_addresses_is_bounded_by_ttl_and_cap` asserts the
  hint SURVIVES `close_peer`. **Session-20 `session-20-hv109b`:
  `test_forced_drop_reconnect_10x` 10/10 (was 0/10 before this fix).** The full
  teardown-then-reconnect recovery also needed HV-107 + HV-109 on top.
- **Found by:** Session-20 bench — `test_forced_drop_reconnect_10x` (regressed
  from Session-13's 10/10 @ ~3.5 s to a permanent stall), `test_reconnect_mesh_cycle`
  (Session-19 2/2 @ ~25–49 s → stuck > 75 s), and the new
  `test_permission_revoke_demotes_then_recovers` (stuck > 75 s). Steady-state
  delivery was unaffected the whole time (`test_pingpong_300char_10x` 10/10,
  `test_sustained_session_recovers_fast` 40/40).
- **Area:** `ble.rs` `close_peer` vs `ensure_accept_poller` (the inbound
  accept-poller's MAC→PeerId resolution) — interacts with HV-13, HV-15, HV-98,
  HV-29, HV-30
- **Severity:** Critical (RETRY / any link drop → inbound half of the link never
  recovers) · **HW gate:** 2 phones

**What (root cause, from `session-20-tier3` logcat):** after P2 does
`stopMesh`+`startMesh` (the RETRY button) or both sides `dropAllLinks`, P1
re-links to P2's GATT server and its ATT writes reach P2's characteristic
(`onCharacteristicWriteRequest matchesIris=true len=294`, every ~8 s) — but P2
emits **no `msg.delivered`**. HV-13's `close_peer` had just evicted
`(P1-MAC → P1-candidate-PeerId)` from `known_addresses`. P2's inbound
accept-poller (`ensure_accept_poller`) resolves an incoming MAC to a PeerId
*only* through `known_addresses`; with the hint gone and P2's own next scan not
yet having re-seen P1's beacon, it falls back to
`synthesize_unknown_peer_id(mac)`. The reassembled inbound stream is then keyed
by a synthetic id, the per-peer poller map is inconsistent (old real-keyed
poller never aborted, new synthetic-keyed poller races), and delivery to the
engine never completes. Session 13 recovered in ~3.5 s precisely *because* this
hint survived the drop.

**Why HV-13 added it:** to stop `known_addresses` growing forever and to drop a
MAC the OS may later rotate to another device. Both are already covered by the
TTL (300 s) + cap (128) sweep in `discover_peers` — a rotated MAC's stale hint
expires on its own within 5 min, and a candidate id is explicitly *not* a trust
boundary (the inbound envelope is signed by its real sender).

**Fix:** `close_peer` no longer touches `known_addresses`; the TTL+cap sweep is
the sole bound. Verify: forced-drop 10/10 back under ~15 s, RETRY 5/5, permission
revoke 3/3.

**Session-20 re-run after the fix:** forced-drop **7/10** (was 0/10), RETRY still
**0/5**, permission still **0/3** — a *second* bug in the same path → **HV-106**.

---

### HV-106 — the inbound accept-poller is gated on `start_advertising` succeeding

- **Fix status:** 🟢 HW-VERIFIED · commit `618e673` · 2026-09-04 · Session 20 ·
  hoisted `ensure_accept_poller` in `BleTransport::start_advertising` to *before*
  the fallible `adapter.start_advertising()` call. L1
  `hv106_accept_poller_runs_even_when_start_advertising_fails`. Verified as part
  of the RETRY / forced-drop / BT-toggle / permission suite passing
  (`session-20-hv109b`, all green).
- **Found by:** Session-20 `test_reconnect_mesh_cycle` / `test_permission_revoke…`
  still stuck after the HV-105 fix.
- **Area:** `ble.rs` `start_advertising` ordering vs `ensure_accept_poller`
- **Severity:** Critical (RETRY = the user's main recovery tool, HV-60) ·
  **HW gate:** 2 phones

**What (from `session-20-hv105/FAIL-test_reconnect_mesh_cycle` logcat):** after
P2 `startMesh`, its `adapter.start_advertising()` transiently fails (the OEM
advertiser is briefly busy right after the preceding `stopAdvertising`;
`ble.advertise_watchdog` fires). `start_advertising` returns `Err` via `?` at
the `.map_err(demote_on_fatal)` line — **before** it reaches
`self.ensure_accept_poller(adapter)`. So no accept-poller is ever spawned for
the fresh engine. P1 (which never disconnected — the OEM GATT server does not
fire a server-side disconnect, HV-93/94) keeps writing; every frame reaches
`onCharacteristicWriteRequest matchesIris=true` and is queued in the Kotlin
adapter's `pendingGattWrites`, but nothing on the core side drains it. No
`msg.delivered`, ever. The advertise-watchdog re-drives *advertising* but not
the accept path, so it never self-heals.

**Fix:** accepting inbound connections needs only `self.adapter()` (already
resolved), not a live outbound advertisement. `ensure_accept_poller` now runs
first; it is idempotent so the ordering change is safe.

**Session-20 re-run after HV-105 + HV-106:** forced-drop **flaky 1–7/10**, RETRY
**0/5**, permission **0/3**. A *third* layer → **HV-107**.

---

### HV-107 — the inbound accept-poller wrote a peripheral-role handle into `connections`, blocking the peer's own outbound `connect()`

- **Fix status:** 🟢 HW-VERIFIED · commit `aa85916` · 2026-09-04 · Session 20 ·
  **fix (a)** — new `inbound_handles: HashMap<PeerId, GattHandle>`, strictly
  separate from `connections` (outbound client links only). The accept-poller
  records inbound handles there; `connect()` / `send()` / discovery consult only
  `connections`, so after a peer restart they correctly see "no outbound link"
  and dial. `close_peer` / `shutdown` / `drop_all_links` / the BT-off handler
  tear down both. L1
  `hv107_inbound_accept_does_not_populate_connections_or_block_the_outbound_dial`.
  **Session-20 `session-20-hv107-fix`: `test_reconnect_mesh_cycle` (RETRY) 5/5,
  recovery 0.3–0.7 s** (was a permanent stall). Full recovery in the
  only-one-side-restarts case (permission revoke) also needed **HV-109**
  (persistent GATT server). HV-108 (handle-keyed pollers) was tried and
  abandoned — it regressed RETRY.
- **Area:** `ble.rs` `ensure_accept_poller` (`connections.entry(peer_id).or_insert`
  with `accepted.handle`) vs `connect()` / `send()` / discovery's idempotent
  `connect_ok`
- **Severity:** Critical · **HW gate:** 2 phones

**What (fully instrumented, `session-20-hv107-trace`):** after P2 does
`stopMesh`+`startMesh` (RETRY), P1 keeps its central→peripheral GATT link to P2
(the OEM GATT server does not fire a disconnect — HV-93/94). P2's fresh-engine
accept-poller correctly sees the inbound connection
(`ble.accept_new_connection`), spawns a reassembly poller, drains P1's frames
(`ble.inbound_drain frames=1`), pushes them to the engine
(`ble.inbound_delivered`), and the engine **does** deliver
(`msg.delivered` fired on P2). **The break is the reply/ACK direction.** The
accept-poller also did `connections.entry(peer).or_insert((accepted.handle, …))`
— but `accepted.handle` is a *peripheral-role* handle (`deviceHash(MAC)`, the
GATT-server key), not a client handle P2 can `gatt_write` on. Now:
1. P2's discovery sees `connections` already has the peer → `discovery.connect_ok`
   "idempotent no-op" → P2 **never dials P1 as a client**.
2. When P2's engine tries to send the delivery ACK, `send()` resolves to that
   entry, calls `gatt_write(deviceHash(MAC), …)`, Kotlin can't find it in the
   client `gattHandles` map → `NotConnected`.
3. P1's `awaitDelivered` never receives the ACK → `send_and_await` times out
   even though P2 got the message.

The HW-9 design comment on that `or_insert` explicitly assumes "a `connect()` to
this peer inserts a real value first and this is a no-op" — which holds on a cold
symmetric bring-up but **not** when one side keeps a live inbound link across the
other's restart.

**Fix options (for a focused session):**
- (a) give the accept-poller its own `inbound_handles: HashMap<PeerId, GattHandle>`
  map used only by the reassembly poller; keep `connections` for client links
  only. `connect()`/`send()`/`discovery` then correctly see "no outbound link"
  and dial.
- (b) tag the inbound entry (sentinel MTU or a separate flag) so `connect()`
  treats it as "receive-only, still needs an outbound dial" and `send()` prefers
  a real client handle.
- (c) on `start_advertising` after a restart, proactively `close_peer` +
  re-`connect()` every peer that only has an inbound handle.

**Resolution (Session 20):** the BLE reconnect cluster took three coordinated
fixes — **HV-105** (keep the MAC hint across `close_peer`), **HV-107** (inbound
handles out of `connections`), **HV-109** (one process-wide GATT server). With
all three: RETRY 5/5, forced-drop 10/10, permission 3/3, BT-toggle 3/3.

---

### HV-108 — (abandoned) handle-keyed inbound reassembly tasks

- **Fix status:** ⬜ N/A — tried and reverted. Keying the accept-poller's
  reassembly tasks by `GattHandle` (to stop a PeerId collision aborting a
  sibling) **regressed RETRY** (`test_reconnect_mesh_cycle` 5/5 → flaky/fail).
  The real root cause of the residual failure was the double GATT server
  (**HV-109**), not the poller keying. Kept here so the approach is not retried.

---

### HV-109 — the GATT server was closed + reopened on every mesh restart, leaking `serverIf` and stranding the peer

- **Fix status:** 🟢 HW-VERIFIED · commit `408f3d3` · 2026-09-04 · Session 20 ·
  the `BluetoothGattServer`, its inbound queues, its callback and `deviceHash()`
  are now **companion-object statics** — one server for the life of the process,
  opened once (`ensureSharedGattServer`), never closed on a mesh stop. Only a
  real BT-off closes it (the OS invalidates it anyway).
- **Severity:** Critical · **HW gate:** 2 phones, restart one side only

**What:** the snippet (and the real app's reconnect path) builds a **new**
`AndroidBleTransportAdapter` on every `startMesh`, and `stopAdvertising` used to
`gattServer.close()` the old one. So a `stopMesh`+`startMesh` left **two**
registered GATT servers (logcat: `serverIf` 8 alive next to 12). A remote
central — which never gets a disconnect when a peripheral closes its server
(Bluetooth core spec: only the central can drop the ACL) — kept writing into the
**old** adapter's queue, which nothing drained. `P1→P2` delivery was permanently
dead after any peer restart where P1 kept its link (RETRY when only P2 restarts;
permission re-grant). Closing+reopening also strands the central against a
changed GATT database with no Service Changed (`0x2A05`) indication —
[JimmyIoT: Service Change on BLE GATT Table](https://jimmywongiot.com/2021/05/25/service-change-on-ble-gatt-table/),
[Martijn van Welie, "Making Android BLE work — part 2"](https://medium.com/@martijn.van.welie/making-android-ble-work-part-2-47a3cdaade07).

**Fix:** one process-wide server, never torn down by a mesh restart. HV-97's
"second stale server" is now structurally impossible. Verified
(`session-20-hv109` / `-hv109b`): permission 3/3, RETRY 5/5, forced-drop 10/10,
BT-toggle 3/3.

---

**§2 Tier-3→Tier-2 gate: ✅ MET (2026-09-04, Session 20).** Every recovery path
verified on hardware with the HV-105 + HV-107 + HV-109 fix stack:
`test_reconnect_mesh_cycle` (RETRY) 5/5, `test_forced_drop_reconnect_10x`
(forced link loss) 10/10, `test_permission_revoke_demotes_then_recovers`
(permission revoke + re-grant) 3/3, `test_bluetooth_toggle_recovers` (BT
off→on) 3/3 all < 60 s, `test_doze_survival` (HV-28) delivered through an 8-min
idle. Airplane mode is a mechanical superset of the BT toggle. **Proceeding to
Tier 2.**

---

## Tier 3 — Connection lifecycle

**§2 Tier-3→Tier-2 gate: ✅ MET (2026-09-04, Session 20).**

Session 19 declared this gate "functionally met" on a 45 s-budget RETRY run; the
owed 10× bench runs in Session 20 showed that was **wrong** — the whole BLE
teardown-then-reconnect path had regressed to a permanent stall (steady-state was
fine, so short runs missed it). It took three coordinated fixes:
- **HV-105** — `close_peer` was evicting the MAC→candidate hint the inbound
  accept-poller needs.
- **HV-107** — the accept-poller wrote a peripheral-role handle into
  `connections`, so after a peer restart this node never dialed back out.
- **HV-109** — `stopMesh`/`startMesh` closed + reopened the GATT server, leaking
  the old `serverIf`; a central that kept its link wrote into a dead queue.

With the full stack (`session-20-hv109` / `-hv109b`, P1 vivo V2205 / P2 vivo 2004):

| recovery path | test | result |
|---|---|---|
| RETRY button (HV-29) | `test_reconnect_mesh_cycle` | **5/5**, 0.4–32.8 s |
| forced link loss (HV-15/105) | `test_forced_drop_reconnect_10x` | **10/10**, 3.3–10.5 s |
| permission revoke + re-grant (HV-30) | `test_permission_revoke_demotes_then_recovers` | **3/3**, 0.7–40.9 s |
| Bluetooth off→on (HV-31) | `test_bluetooth_toggle_recovers` | **3/3**, all < 60 s |
| Doze / 8-min idle (HV-28) | `test_doze_survival` | **PASS** (Session 19) |

Airplane mode is a mechanical superset of the BT toggle. **Proceeding to Tier 2.**

---

## Tier 2 — Wi-Fi Direct reliability & group-owner conflict

> **Hard requirement (operator, 2026-09-05, Session 24): no OS-level "Invitation
> to connect" prompt may ever reach the user.** While diagnosing the discovery
> blocker, a native Android Wi-Fi Direct connection was observed forming
> between the two bench phones with a system "Invitation" UI visible on
> screen. The operator has stated this must never be user-facing in the real
> app — the connection has to be negotiated and accepted entirely by IRIS's
> own code, silently, the same way the BLE control-plane link already is. Any
> implementation of HV-19/21/22's `connect()` path that would surface a stock
> "Invitation to connect" / "Invitation received" system dialog is a bug to
> eliminate, not a UX gap to accept. Concretely: IRIS already calls
> `WifiP2pManager.connect()` programmatically (never the OS's own peer-picker
> UI), and a plain `WifiP2pConfig` (no `wpsInfo.setup` override) uses the
> **push-button** WPS path by default, which on stock AOSP does **not**
> require a manual per-connection tap — the system dialog seen during
> diagnosis came from *Android's own Wi-Fi Direct Settings screen*, a
> different, user-driven code path this app never uses. This must be
> explicitly verified once HV-21's fix lands: drive a cold-start connection
> through IRIS's own `connect()`/`createGroup()` calls only (app fully
> backgrounded from the user's perspective, no Settings screen involved) and
> confirm via screenshot/logcat that no system invitation dialog appears at
> any point. If one does appear, that is a new, blocking finding — treat
> "user must tap nothing to connect" as a pass/fail criterion of the Tier-2
> hardware gate from this session forward, not a nice-to-have.

> **Tier-2 gate status (Session 22, 2026-09-05): 🔒 BLOCKED, not closed.**
> Loop §2 gate: "cold-start both phones 10× (alternating power-on order) →
> exactly one group forms → messages flow both ways every time." HV-19's
> election and HV-22's re-formation are implemented and pass all L1 tests
> (27 `wifi_direct` unit tests, 796/796 `iris-core` total), but **cannot be
> hardware-verified**: Wi-Fi Direct DNS-SD discovery finds `peers_seen=0` on
> both bench phones in every attempt, so no group is ever reachable for the
> election/re-formation logic to act on. Two real hardware attempts were
> made this session (both failed the same way): a 2-cycle cold-start run
> after enabling Wi-Fi (was off on both phones — fixed, but did not resolve
> discovery) and a second 2-cycle run after a full radio power-cycle
> (`svc wifi disable`/`enable`, confirmed via `dumpsys wifi`). A programmatic
> attempt to clear P1's two persisted P2P groups (`dumpsys wifip2p`'s
> `mGroups`, one of which references P2's exact device model — the leading
> suspect for interference) via reflection into the hidden
> `WifiP2pManager.requestPersistentGroupInfo`/`deletePersistentGroup` APIs
> was attempted and abandoned: the call completes without error but returns
> an empty group list to a non-privileged app despite
> `dumpsys wifip2p`'s own `mWifiP2pStatsProto.numPersistentGroup=2` — i.e.
> the platform silently withholds this data from a normal app, so this is
> not a viable adb/code path (the experiment was reverted, not committed).
> **Root cause is now attributed to HV-21** (DNS-SD listeners never fire on
> either phone — evidence below) rather than HV-25 concurrency (ruled out as
> the *sole* cause — see HV-25 note) or the persisted-group hypothesis
> (blocked from confirmation, see above). Per the loop's 3-attempts rule this
> is 2 of 3 permitted attempts on the same underlying blocker before a formal
> escalation; the next step is HV-21's own fix sketch (carry identity over
> the BLE control plane instead of depending on P2P DNS-SD TXT), not another
> blind retry of cold-start. HV-19/HV-20/HV-22 code changes are being
> committed on the strength of L1 verification with this HW-PENDING status
> recorded, per the loop's own allowance for a blocked-but-documented finding
> — they are not being marked 🟢.
>
> **Update (same session, later): root cause is platform/OEM-level, not an
> IRIS bug.** To isolate whether IRIS's own DNS-SD code path was at fault, we
> opened Android's own built-in Wi-Fi Direct settings screen on both phones
> (`com.android.settings/.Settings$WifiP2pSettingsActivity` — found via
> `dumpsys package com.android.settings | grep -i p2p`) — this uses the
> stock AOSP `discoverPeers()`/`PeerListListener` path with **zero IRIS code
> involved**. Result: `dumpsys wifip2p`'s `mDiscoveryStarted` went `true` on
> both phones, `numTotalPeerScans` incremented (proving a real scan ran), but
> after 35+ seconds of waiting **no `mPeers`/device-list section ever
> appeared in the dump on either phone** — the stock Android P2P UI finds
> zero peers between these two specific phones, the same symptom IRIS sees.
> This reframes the blocker: it is very unlikely to be fixable by any
> IRIS-side code change (HV-21's BLE-carries-identity fix sketch would still
> leave discovery itself broken, since discovery is what finds the peer to
> connect to in the first place — carrying identity differently doesn't
> conjure a peer that the radio never sees). The remaining candidates are
> environmental/hardware, outside this loop's code-fix scope: (a) an OEM
> (vivo OriginOS) P2P stack restriction not yet identified, (b) a genuine
> RF/channel incompatibility between these two specific phones' Wi-Fi Direct
> radios, (c) regulatory-domain or country-code mismatch affecting P2P social
> channels, or (d) a firmware bug requiring an OS update. **Recommended next
> steps for the operator** (not adb-automatable): check for a pending
> vivo/OriginOS system update on either phone; try the two phones extremely
> close (<0.3 m, no obstruction) in case of a weak P2P antenna path; if
> possible, test one of these two phones' Wi-Fi Direct against a **third,
> different-OEM** phone to isolate whether the fault is phone-specific or
> pairwise. Tier-2 hardware verification remains blocked pending one of
> these; continuing autonomously into Tier 8 (Shell UX) per §2's explicit
> allowance to work it in parallel, since it does not depend on this
> blocker.
>
> **SUPERSEDED (same session, later): the above 35-second observation was a
> false negative — the platform-level conclusion was premature.** After
> closing Tier 8 work, the two phones' Wi-Fi Direct settings screens (left
> open in the background) were found to have **actually connected** —
> `dumpsys wifip2p` showed a real populated peer list on both sides and P2
> briefly reached `groupFormed: true isGroupOwner: true groupOwnerAddress:
> 192.168.49.x`, a genuine live Wi-Fi Direct link, using the plain
> `discoverPeers()` API. So the radios, drivers, and regulatory state are
> fine — peer discovery **does** work on this hardware, it is just far
> slower than the 35 s this session originally waited (measured in minutes,
> not seconds, for this specific hardware/environment). Re-testing IRIS
> immediately afterward (fresh relaunch, clean logcat) still showed
> `wifi-direct-0 peers_seen=0` right away — confirming the blocker is
> specifically **IRIS's DNS-SD service-discovery path**
> (`discoverServices`/`addServiceRequest`/TXT records), not peer discovery
> in general. See HV-21's entry for the full evidence and the now-confirmed
> fix direction: switch to `discoverPeers()`/`requestPeers()` and carry
> identity over BLE instead of DNS-SD TXT. This is real, scoped follow-up
> work for a future session — implementing it was not attempted this
> session (time/turn budget), but the path forward is no longer a guess.

### HV-19 — Two peers both call `createGroup` → GO/GO conflict; there is no election

- **Fix status:** ✅ Fixed · HW-PENDING (blocked, see below) · commit `<pending>`
  · 2026-09-04 · Session 22 · **deterministic PeerId election.**
  `WifiDirectTransport` now stashes its own PeerId (`local_peer_id`, from
  `start_advertising`); `connect()`'s new `should_be_go(peer)` compares it
  against the peer's candidate id (both converted to the same `peer_short`
  candidate form — HW-17/DEC-WD-0007 — so it's a fair comparison) and exactly
  one side creates. An explicit strong intent (`go_intent` above/below
  `GO_INTENT_BALANCED`, e.g. fixed infra = 14, never-GO = 0) is still honoured
  verbatim; the election only runs for the common balanced-vs-balanced default.
  L1 `hv19_two_default_config_peers_elect_exactly_one_go` +
  `hv19_election_is_symmetric_by_peer_id` (swap which id is higher, confirm the
  *other* physical device becomes GO — HV-19's own "symmetry bugs only show
  when you swap" note). Research:
  [WifiP2pManager createGroup GO election is a known hard problem for apps](https://patents.justia.com/patent/20150163300)
  ("It may be difficult to decide which device is to become the GO... users
  need to make decisions"); the standard fix (multiple sources, IRIS's own
  Session-19 research) is exactly this deterministic-comparison pattern.
  **HW verification BLOCKED** — see the Tier-2 gate note below `## Tier 2`:
  Wi-Fi Direct DNS-SD discovery finds **zero peers** on this bench, on both
  phones, for an unrelated reason (HV-21/HV-25 territory) — the election logic
  this fixes cannot even be reached until discovery itself works.
- **Area:** `AndroidWifiDirectTransportAdapter.createGroup` /
  `createGroupWithBand`, `wifi_direct.rs` (GO intent), `WifiDirectTransport`
- **Severity:** Critical · **HW gate:** 2 phones

**What:** `createGroupWithBand` unconditionally builds an autonomous GO
(`WifiP2pConfig.Builder().setNetworkName(groupNetworkName)...`) with **per-adapter
random credentials** (`DIRECT-ir-iris%04x` + a random passphrase generated per
instance). If both phones decide to `createGroup` (which the core does when it
wants a data plane and neither is a client), each becomes the owner of its **own**
group with its own SSID/passphrase, and they can never join each other. The core
has `GO_INTENT_BALANCED = 7` and a `GroupConfig.go_intent` field, but the Android
adapter's `createGroup` path ignores intent entirely — it always makes a named
autonomous GO. `connect()`-based negotiation (which *does* do GO election via
`WifiP2pConfig.deviceAddress`) is only used by `joinGroup`/`addClient`.

**Why it matters:** this is precisely the operator's "the Wi-Fi Direct connections
create conflict." Symmetric peers both self-elect as GO and the transport is dead.

**Fix sketch (research — this is the big Wi-Fi Direct item):**
1. Decide the topology: does IRIS want **one GO per physical cluster** with others
   as clients (standard Wi-Fi Direct), and if so, **who decides**? Options: (a)
   lexicographically-lowest PeerId in the discovered set forms the group, others
   `connect()` to it; (b) BLE control-plane negotiation picks the GO before any
   Wi-Fi Direct call (Wi-Fi Direct is "the data plane on the BLE control plane"
   per the module doc — use that); (c) use framework `connect()` negotiation
   (GO intent) instead of autonomous `createGroup` for the common case.
2. Only fall back to autonomous `createGroup` when a band pin is required, and
   even then coordinate so exactly one peer does it.
3. Handle the tie: if both form groups, detect it (both see the other still
   advertising, neither sees the other as a client) and have the higher-PeerId
   peer `removeGroup` and `connect` to the lower.

**HW verification:** cold-start both phones; within T seconds exactly one group
exists and both nodes are in it; message flows both ways. Repeat 10×, alternating
which phone powers on first.

---

### HV-20 — Per-instance random group credentials make a persistent GO non-rejoinable

- **Fix status:** ✅ Fixed (narrower than originally scoped) · commit `<pending>`
  · 2026-09-04 · Session 22 · re-reading `createGroupWithBand` at HEAD: the
  custom `setNetworkName`/`setPassphrase` config only builds when a **specific
  band is requested** (`bandId != null`, API 29+) — `GroupConfig::default()`'s
  `band` is `Auto`, so the common default-config path already calls the plain
  `createGroup(channel, listener)` overload, which per Android's own
  documented behaviour IS the framework-managed **persistent** group (this is
  what the deterministic-election test above actually exercises). The
  band-constrained path added `.enablePersistentMode(true)` to its
  `WifiP2pConfig.Builder` for the same guarantee when a band *is* pinned.
  **Not independently HW-tested** — the bench doesn't pin a band, so this
  path isn't exercised by the current tests; low risk, mirrors the documented
  API contract exactly.
- **Area:** `AndroidWifiDirectTransportAdapter.groupNetworkName` / `groupPassphrase`
- **Severity:** High · **HW gate:** 2 phones, restart one

**What:** `groupNetworkName` and `groupPassphrase` are generated once **per
adapter instance** (`SecureRandom` at field init). The doc claims "a re-formed
group keeps a stable identity within the process lifetime" — but across an app
restart, a service restart, or an engine rebuild, the credentials change. The core
treats `persistent: true`. A client that was in the group cannot rejoin after the
GO's app restarts because the SSID/passphrase it knew are gone. And clients join
via `connect(deviceAddress)` negotiation anyway, so the explicit passphrase is
mostly unused — except it forces the *named autonomous GO* path (HV-19).

**Fix sketch:** derive stable group credentials from the node identity (e.g.
`DIRECT-ir-` + a hash of the PeerId, passphrase from a KDF over a stable secret)
so a restarted GO is the same group; or stop using the named-GO path entirely
(see HV-19) so credentials are never needed.

---

### HV-21 — DNS-SD TXT-record discovery is OEM-fragile; the current code has three stacked workarounds and still races

- **Fix status:** ⬜ · **Session 22 (2026-09-05) fresh evidence — root-cause
  confirmed, not yet fixed:** ran the bench with a clean Wi-Fi radio
  (`svc wifi disable`/`enable` on both phones, ruling out "radio just off" —
  it *was* off going in, which explains earlier sessions' "radio switched
  off" transport errors, but turning it on did not fix discovery) and full
  `IrisWifiDirectDiag` logging across a ~3 min multi-cycle run on both
  vivo phones (P1 Android 15, P2 Android 13). Result: `discoverServices()`/
  `addServiceRequest()`/`addLocalService()` all return success (no
  exceptions), but **zero** `IrisWifiDirectDiag` PTR/TXT listener log lines
  fire on *either* phone for the entire session —
  `discovery.scan_pass_complete transport=wifi-direct-0 peers_seen=0` every
  pass. This is stronger and more specific than the original HW-13..16 notes
  (which at least saw PTR fire without TXT): here neither listener fires at
  all, on two same-vendor (vivo) phones, which the original finding's "OEM
  variance" framing did not anticipate. This is the concrete blocker gating
  the whole `## Tier 2` HW-verification gate — see the gate note below.
  **Later same session — root cause conclusively confirmed via a live,
  reproduced connection**: while both phones had Android's own stock Wi-Fi
  Direct settings screen open (left running from an earlier diagnostic),
  `dumpsys wifip2p` showed each phone's **plain peer list actually populate**
  with the other ("vivo Y35" seen from P2's side, "vivo 2004 — Invited" from
  P1's side after a tap sent a connect invitation), and P2 briefly reached
  `mWifiP2pInfo groupFormed: true isGroupOwner: true groupOwnerAddress:
  192.168.49.x` / `mDetailedState: CONNECTED` — a **real, live Wi-Fi Direct
  connection** between these two exact phones, using the plain
  `discoverPeers()`/`PeerListListener` API. P1 stalled at `CONNECTING`
  (a separate, minor negotiation hiccup — not investigated further, not
  the point). Immediately after, with that native connection torn down,
  both phones' IRIS app was relaunched fresh and re-tested: **`wifi-direct-0
  peers_seen=0` on both, immediately** — proving definitively that the
  underlying P2P radios, drivers, and regulatory/channel state are all fine
  and *can* see each other; it is specifically IRIS's **DNS-SD service
  discovery** path (`discoverServices()`/`addServiceRequest()`) that never
  resolves on this hardware, not `discoverPeers()`. This fully vindicates
  HV-21's original hypothesis and fix-sketch option 2 (carry identity over
  the BLE control plane instead of depending on P2P DNS-SD TXT) — that is
  now the confirmed, correct fix, not a guess. The Tier-2 blocker is **not**
  environmental/hardware after all (the earlier "platform/OEM-level, not
  IRIS's" framing recorded elsewhere in this doc is superseded by this
  finding) — it is a real, fixable IRIS defect: stop using
  `discoverServices`/DNS-SD for peer discovery, use `discoverPeers()` +
  `requestPeers()` (plain `WifiP2pManager.PeerListListener`) instead, and
  carry the 22-byte IRIS beacon over BLE (already the identity channel) to
  identify *which* discovered P2P device is an IRIS peer, rather than
  relying on a TXT record this hardware never delivers.
- **Area:** `AndroidWifiDirectTransportAdapter` — `dnsSdServiceListener`,
  `dnsSdTxtRecordListener`, `serviceRequest`, `startDnsSd` (HW-13/14/15/16),
  `pendingServiceOnly` / `pendingTxtBeacons`
- **Severity:** High · **HW gate:** 2–3 phones across ≥2 OEMs

**What:** the comments (HW-13 through HW-16) document a hard-won sequence: the
empty placeholder beacon winning the registration race; the PTR listener firing
but the TXT listener never firing; `newInstance(serviceType)` vs
`newInstance(instanceName, serviceType)`; the two-arg request making the *service*
listener stop firing on some OEMs; `stopPeerDiscovery` invalidating the service
request on at least one device; needing to force a discovery restart when the
beacon changes. This is a pile of empirical patches against undocumented
per-OEM Wi-Fi P2P behaviour. It "works on the three devices tested" but the
operator's fleet is two phones and the behaviour is described as device-specific.

**Why it matters:** "sometimes discovers the peer over Wi-Fi Direct" — DNS-SD TXT
delivery is the single most OEM-variable part of Android Wi-Fi P2P.

**Fix sketch (research):**
1. Re-verify the whole DNS-SD path on the *current* two bench phones with verbose
   `IrisWifiDirectDiag` logging; document exactly which listener fires when on
   each.
2. Evaluate not depending on TXT at all: discover the *service* (PTR) over
   DNS-SD, then carry the 22-byte beacon **over the BLE control plane** (which is
   already the identity channel) or over the first socket frame after `connect`.
   Wi-Fi Direct as "data plane on BLE control plane" (module doc) suggests
   identity should come from BLE, not from a fragile P2P TXT record.
3. If TXT must stay, add a retry/repair loop and a hard timeout that surfaces
   "peer seen but not identified" rather than silently dropping.

---

### HV-22 — `WIFI_P2P_CONNECTION_CHANGED` false → immediate teardown, but no re-formation attempt

- **Fix status:** ✅ Fixed · HW-PENDING (blocked, see below) · commit `<pending>`
  · 2026-09-04 · Session 22. `WifiDirectTransport` gained a `poll_health()`
  override (generic ~5s health-tick call site already used by BLE's HV-99
  fix, confirmed at `discovery/mod.rs:564`): it fetches the adapter's current
  `group_info()` and tears down any link whose handle is no longer the GO or
  a listed client, so a group lost at the OS level (Kotlin's own
  `groupFormed=false` handling clears its cache, but nothing told core) is
  detected and cleared within one tick — re-`connect()` then re-forms it
  (respecting HV-19's election). L1 `hv22_poll_health_clears_a_lost_group_and_reconnect_reforms_it`:
  forms a group, force-drops it via the adapter directly (simulating OS-level
  loss), asserts `poll_health` clears the stale link, then asserts a fresh
  `connect()` succeeds again. **HW verification BLOCKED for the same reason
  as HV-19** — see the Tier-2 gate note below: discovery itself never finds a
  peer on this bench, so no group ever forms in the first place for a
  mid-session loss to be exercised against real hardware.
- **Area:** `AndroidWifiDirectTransportAdapter.p2pStateReceiver`
  (`WIFI_P2P_CONNECTION_CHANGED_ACTION`, `else` branch — HW-21), `closeDataPath`
- **Severity:** High · **HW gate:** 2 phones, move them apart

**What:** HW-21 added handling for `groupFormed == false`: `closeDataPath()`,
`groupState.clear()`, `cachedGoAddr = null`. Good — the stale-link ACK-failure
symptom is addressed. But now the group is simply gone and nothing re-forms it.
The core's `WifiDirectTransport` will drop to `Degraded`/`Unavailable`; whether
anything drives a re-`createGroup`/`connect` depends on the discovery re-arm loop
noticing the peer again. Same gap as BLE HV-15.

**Fix sketch:** on an unexpected group loss, schedule a bounded re-formation
(respecting the HV-19 election), and hold queued frames in the outbox (which
already exists) rather than failing them.

---

### HV-23 — `awaitAction` serializes all `WifiP2pManager` calls on one mutex; a slow platform bring-up blocks everything for up to 30 s each

- **Fix status:** ✅ Fixed (part a only) · HW-PENDING · 2026-09-05 · Session 22.
  `awaitAction`'s BUSY-retry `delay` used to run *inside*
  `actionMutex.withLock { ... }` — a caller retrying a BUSY platform action
  held the mutex for its entire backoff window (up to 2.5 s,
  `BUSY_RETRY_ATTEMPTS × BUSY_RETRY_DELAY_MS`), serializing every other
  Wi-Fi Direct call (including `start`/`startDnsSd`/`startDiscovery`) behind
  it. Restructured so the mutex is acquired fresh per attempt
  (`actionMutex.withLock { awaitActionOnce(...) }` inside the retry loop,
  not wrapping the loop) — still at most one platform action in flight at a
  time (HW-7's constraint, unchanged), but other queued callers can now run
  during another call's BUSY backoff sleep. Parts (b) (gate first action on
  `WIFI_P2P_STATE_ENABLED` instead of spin-retrying `BUSY`) and (c) (surface
  a "warming up" transient state) not attempted this session — lower value
  once (a) removes the worst blocking behaviour, and this bench doesn't
  reproduce the cited Samsung S24 Ultra's 30s+ bring-up stall to verify
  against. **Not independently HW-tested** — no BUSY condition reproducible
  on this (vivo) bench; a correctness fix verified by code inspection and a
  clean rebuild, not by a hardware BUSY-condition test.
- **Area:** `AndroidWifiDirectTransportAdapter.awaitAction` / `actionMutex` /
  BUSY retry (HW-11), `FfiCallTimeout`
- **Severity:** Medium · **HW gate:** Samsung-class device (HW-11 cites S24 Ultra)

**What:** HW-11 found that `WifiP2pManager` tolerates one outstanding action per
channel, so every call now goes through `actionMutex`. It also found that on a
real S24 Ultra the platform's own P2P state machine takes "well over 30 s" to
leave `P2pDisabledState`, so the first several actions get `BUSY` and are retried
5× with a 500 ms gap. Combined: during platform bring-up, every Wi-Fi Direct
operation (including `start`, `startDnsSd`, `startDiscovery`) is serialized behind
a call that may `BUSY`-retry for ~2.5 s and then, if the platform is still not
ready, fail — and the 30 s `FfiCallTimeout` per call means a stuck action can
block the queue for the full budget.

**Why it matters:** startup is when the user is staring at the screen. If Wi-Fi
Direct takes 30–60 s to become usable and blocks its own init calls, the app feels
broken even when it will eventually work.

**Fix sketch:** (a) increase BUSY retry patience but make it non-blocking (don't
hold `actionMutex` during the backoff `delay` — release and re-acquire); (b)
detect `WIFI_P2P_STATE_ENABLED` from the broadcast and gate the first action on
it rather than spin-retrying `BUSY`; (c) surface "Wi-Fi Direct warming up" as a
transient state so the UI/other transports proceed without it.

---

### HV-24 — Fixed GO port 47631 + `SO_REUSEADDR`: verify no collision with other apps and clean recovery after crash

- **Fix status:** ⬜
- **Area:** `SocketDataPath.FramedSocketServer.WIFI_DIRECT_GO_PORT = 47631`,
  `ensureGroupServer`
- **Severity:** Low–Medium · **HW gate:** 2 phones, kill the GO app mid-session

**What:** the GO listens on a hard-coded 47631. Wi-Fi Direct gives clients the
GO's IP but no port channel, so a fixed port is reasonable — but (a) another app on
the GO phone could hold 47631; (b) after a hard crash the socket may be in a state
`SO_REUSEADDR` doesn't fully recover from on all stacks; (c) there is no
health-check that the client's dial actually reached IRIS and not some other
listener.

**Fix sketch:** add a 4-byte magic + version handshake (partly there — the MAC
handshake frame) that both ends validate before trusting the socket; on bind
failure, try a small set of fallback ports and advertise the chosen one over the
BLE control plane or the group's TXT record; add a "socket up, handshake OK"
diagnostic.

---

### HV-25 — Single-radio STA + P2P concurrency: IRIS Wi-Fi Direct vs the user's Wi-Fi connection

- **Fix status:** ⬜
- **Area:** `WifiDirectTransport` availability/`Degraded` handling,
  `AndroidWifiDirectTransportAdapter.availability`
- **Severity:** Medium · **HW gate:** 2 phones, one on a Wi-Fi network

**What:** many phones have a single 2.4/5 GHz radio shared between station mode
(the user's home/office Wi-Fi) and Wi-Fi Direct. Forming a P2P group can force the
STA link to the P2P channel, drop the user's internet, or fail outright if the STA
is on an incompatible channel (DFS, regulatory). The module doc acknowledges this
("single-radio STA+P2P churn → Degraded"). It is not tested and there is no policy
for "the user is on Wi-Fi and wants to keep it."

**Session 22 (2026-09-05) evidence — concurrency is NOT the sole explanation:**
during the HV-19/21/22 bench run, P1 was STA-associated to a home AP
("Taksh tirth", 2417 MHz) the whole time while P2 had no STA association at
all — yet **both** phones still saw `peers_seen=0`. If single-radio STA+P2P
churn were the full story, P2 (no STA link to protect) should have discovered
fine. It didn't. This doesn't rule HV-25 out as *a* contributing factor on
P1, but it means HV-25 alone cannot explain the bench's zero-discovery
blocker — HV-21 (DNS-SD listeners never firing on either phone) is the
primary, better-evidenced cause.

**Follow-up test (operator suggestion, same session):** operator had P1
forget the "Taksh tirth" network entirely (not just disconnect) and asked to
re-test. Re-ran with both phones confirmed STA-disconnected
(`dumpsys wifi`: P1 `DISCONNECTED`/no active network, P2 already had none).
Result: **unchanged** — `wifi-direct-0 peers_seen=0` on both phones for the
whole run. The one message that *did* get marked delivered in this run went
over `ble-android` (`meshSnapshot` shows `"links":["ble-android:Good"]`), not
Wi-Fi Direct — confirming this was a BLE fallback delivery, not a Wi-Fi
Direct connection, and that Wi-Fi Direct discovery is broken independent of
STA/AP association state. This closes out HV-25 as a contributing-cause
candidate for the current blocker: it may still matter in other scenarios,
but it is not why this bench sees zero Wi-Fi Direct peers.

**Fix sketch:** query `WifiManager.isStaConcurrencyForMultiInternetSupported` /
`isP2pSupported` and the STA channel; prefer BLE when P2P would disrupt an active
STA link the user cares about; document the trade-off. Bench: does forming an
IRIS group kill the user's Wi-Fi on each phone.

---

### HV-26 — GO-side handshake read is bounded but the accept loop is single-threaded through one blocking read

- **Fix status:** ⬜
- **Area:** `AndroidWifiDirectTransportAdapter.ensureGroupServer` →
  `FramedSocketServer.onAccepted` → `readHandshakeFrame` (5 s `soTimeout`)
- **Severity:** Low–Medium · **HW gate:** 3 phones (GO + 2 clients)

**What:** `FramedSocketServer`'s accept loop calls `onAccepted(socket)` inline; for
Wi-Fi Direct that callback does a **blocking** `readHandshakeFrame` (up to 5 s)
before `callbackScope.launch { attachLink() }`. So while the GO waits up to 5 s
for client A's handshake, client B's connection sits unaccepted. With the
`MAX_CONCURRENT_LINKS = 16` cap and a mesh of a few clients this is survivable,
but a client that connects and stalls delays every subsequent client by 5 s.

**Fix sketch:** move the handshake read into the launched coroutine (accept →
launch → read-with-timeout → attach), keeping the slot accounting correct.

---

## Tier 3 — Connection lifecycle: drop, backoff, reconnect, coexistence

### HV-27 — There is no "connection health" concept — a link is either in the map or not

- **Fix status:** ✅ Fixed · HW-PENDING · commit 4a2889b · 2026-09-04 · Session
  18 · §5 group with HV-48 · (a) `Transport::link_quality(peer)` — `BleTransport`
  tracks `link_activity` (bumped per successful `send`, cleared by `close_peer`),
  maps `elapsed()` → Good ≤15 s / Fair ≤45 s / Poor; `discovery` upserts the
  real value so `/diag` shows per-link health instead of a hardcoded `Good`.
  (b) `AndroidBleTransportAdapter.ensureLivenessTick()` — 7 s cross-check of live
  handles against `getConnectedDevices(GATT)`; a handle the stack dropped →
  `pendingDisconnectedHandles` (HV-94 drain) — catches the silent death the OEM
  disconnect callback misses. L1 `hv27_link_quality_tracks_recent_activity`.
  **Not done:** an application keepalive frame (protocol change) — a genuinely
  idle link still can't be proactively probed without traffic; the liveness tick
  + Android's ~20 s LL supervision timeout cover the "peer gone" case.
  **HW-pending:** verify the tick logs `liveness: handle N no longer
  stack-connected` and `close_peer`s faster than a failed send.
- **Was:** ⬜
- **Area:** `ble.rs` `link_activity` / `link_quality`, `discovery/mod.rs`,
  `AndroidBleTransportAdapter.ensureLivenessTick`, `manager.rs` (HV-48)
- **Severity:** High · **HW gate:** 2 phones

**What:** a GATT/socket link is binary: present in `connections`/`links` or
removed. There is no keepalive, no RTT/loss tracking per link, no "this link has
missed 3 keepalives, mark it suspect." So a link that is *silently* dead (peer
walked away, radio glitched, no disconnect callback fired — common on Android BLE)
stays "connected," the manager keeps selecting it, every send fails, and only the
Nth write failure's `close_peer` finally removes it. `EwmaGoodput` exists on the
transport but is per-transport, not per-link, and is not used to demote a
specific bad link.

**Fix sketch:** per-link keepalive (a tiny periodic frame or a GATT read),
per-link EWMA loss/RTT, and a "suspect → probe → drop" state machine so a dead
link is detected in seconds, not on the next user send. Feed per-link health into
the manager so a healthy link is preferred over a suspect one of the same
transport.

---

### HV-28 — `DiscoveryManager` cadence, `poll_interval`, and the three per-transport pollers vs Doze / battery

- **Fix status:** ✅ Delivery-under-Doze HW-verified · commit 56c0757 · 2026-09-04 · Session 19 · P1=vivo V2205 (Android 15) P2=vivo 2004 (Android 13)
  · `test_doze_survival`: both phones screen-off + `battery unplug` + light Doze,
  a P2→P1 message sent *during* an 8-min idle window **delivered**, post-wake
  round-trip 391 ms, 0 `msg.delivery_failed`. The acute risk (Doze throttles the
  pollers → inbound frames TTL-expire) is clear — the battery-opt-exempt
  `connectedDevice` FGS keeps them alive. **Deferred:** the battery profile
  (needs an EXP-003 measurement rig) + consolidating the per-peer BLE inbound
  pollers into one transport-wide poller (a dense-mesh optimisation, 3+ phones);
  no code change this session. `deep`-Doze `force-idle` suspends USB adb on these
  OEMs — light Doze is the FGS-exempt reality anyway.
- **Was:** ⬜
- **Area:** `ble.rs` `spawn_inbound_poller` (poller consolidation, deferred),
  `IrisBleService`, EXP-003 battery rig
- **Severity:** Medium · **HW gate:** 2 phones screen-off (done); battery
  measurement (deferred, EXP-003)

**What:** at rest the node runs: the outbound delivery loop every `poll_interval`;
one BLE inbound poller per connected peer at 50 ms (active) / 500 ms (idle); the
BLE accept-poller at 500 ms; the Wi-Fi Direct poller at 500 ms; the Wi-Fi Aware
poller; the discovery scan loop on ~30 s; a 15-min WorkManager relay drain. Under
Doze, the foreground service keeps the CPU available but the radios are still
subject to OS throttling, and the wakeup cadence has never been battery-profiled
on a real device. Conversely, if Doze throttles the pollers, inbound frames sit
undrained and messages TTL-expire.

**Fix sketch:** profile battery for 30 min screen-off with an idle 2-node mesh;
consolidate the BLE per-peer pollers into one transport-wide poller (the Wi-Fi
Aware transport already did this per a GAP-12 comment); align cadences; make the
idle poll interval adaptive to charging state and battery level; verify inbound
delivery latency under Doze.

---

### HV-29 — `stop_all` / `start_all` / `reconnectMesh` cycle: verify the radios actually come back

- **Fix status:** 🟢 HW-VERIFIED · commit b604c16 (+ HV-105/107/109) · 2026-09-04
  · Session 20 · the engine bug: `stop_all` → `MessageEngine::shutdown` aborts
  the delivery/ack/gc loops; `start_all` never revived them → RETRY = UI RUNNING,
  dead engine. Fix: an idempotent `MessageEngine::restart()`;
  `IrisEngine::start_all` calls it + `discovery.wake()`. L1
  `hv29_restart_after_shutdown_revives_the_background_loops`. The RETRY path also
  needed the BLE reconnect stack (HV-105 + HV-107 + HV-109) — Session 19's "2/2"
  was a 45 s budget that the send-retry masked. **Session-20 `session-20-hv109b`:
  `test_reconnect_mesh_cycle` 5/5, recovery 0.4–32.8 s, 0 `msg.delivery_failed`.**
- **Was:** ⬜
- **Area:** `iris-core` `MessageEngine::{shutdown,restart}`, `engine.rs`
  `start_all` / `stop_all`, `MeshViewModel.reconnectMesh`, `MeshRepository`
- **Severity:** High · **HW gate:** 2 phones, tap "RETRY" repeatedly

**What:** `reconnectMesh()` calls `stopMesh()` (which reaches
`MessageEngine::shutdown()` — a *terminal* operation on a `@Singleton` engine per
the `onCleared` comment) then `ensureStarted()`. If `shutdown()` is truly
terminal, the re-`startAll()` runs against a dead engine and the UI reports
RUNNING with nothing alive — the exact bug the `onCleared` comment says was fixed
for rotation but which `reconnectMesh` re-introduces. The RetryNotice in
`ConsoleScreen` calls `reconnectMesh()` on the UNAVAILABLE path — the one the user
hits most.

**Fix sketch:** confirm whether `MessageEngine::shutdown` is reusable; if not,
`reconnectMesh` must rebuild the engine (new `IrisEngine`), which means the
`@Singleton` + `Lazy<IrisEngine>` wiring needs a recreate path. Bench: trigger
UNAVAILABLE (turn Bluetooth off), turn it back on, tap RETRY, confirm messages
flow again.

---

### HV-30 — Permission revocation mid-session (BLUETOOTH_SCAN / NEARBY_WIFI_DEVICES)

- **Fix status:** 🟢 HW-VERIFIED · commit 79866b5 (+ HV-109) · 2026-09-04 ·
  Session 20 · `BleTransport::demote_on_fatal()` → `Unavailable` on
  `PermissionDenied`/`RadioDisabled`/`HardwareUnavailable` from scan/advertise
  (was: only `set_mtu` did) so `select_transports` drops the dark transport and
  the UNAVAILABLE → `RetryNotice` path fires; `ConsoleScreen`
  `LifecycleResumeEffect` re-reads permissions every resume so a Settings-revoke
  surfaces the `PermissionNotice`. L1
  `hv30_revoked_permission_marks_the_transport_unavailable_and_recovers`.
  **Session-20 `session-20-hv109`: `test_permission_revoke_demotes_then_recovers`
  3/3** — `appops` revoke of `bluetooth_scan` demotes the BLE transport within
  0 s, re-grant + `stopMesh`/`startMesh` recovers delivery in 0.7 / 32.8 / 40.9 s,
  0 `msg.delivery_failed`. The recovery half depended on **HV-109** (persistent
  GATT server) — before it, a re-grant left the peer that had kept its link
  writing into a dead queue.
- **Was:** ⬜
- **Area:** `ble.rs::demote_on_fatal`, `ConsoleScreen` permission re-check,
  `PermissionNotice`
- **Severity:** Medium · **HW gate:** 2 phones, revoke in Settings mid-session

**What:** the adapters map `SecurityException` → `PermissionDenied` (good), but the
UI only checks permissions at composition and after a request result. If the user
revokes a grant from Settings while the app runs, every radio call starts throwing
`PermissionDenied`, the transports go dark, and the UI still shows LINK/RUNNING
until something forces a recheck.

**Fix sketch:** re-check permissions on `ON_RESUME`; on a `PermissionDenied` from
any transport, surface the PermissionNotice and set status to a distinct
"permission lost" state.

---

### HV-31 — Airplane mode / Bluetooth toggle / Wi-Fi toggle: state-machine recovery

- **Fix status:** 🟢 HW-VERIFIED · commit 0875136 (+ HV-99/107/109) · 2026-09-04
  · Session 20 · `btStateReceiver` for `BluetoothAdapter.ACTION_STATE_CHANGED`
  (mirrors the Wi-Fi Direct `p2pStateReceiver`) + HV-95 advertise watchdog +
  HV-99 bounded reconnect probe. **Session-20 `session-20-hv109b`:
  `test_bluetooth_toggle_recovers` 3/3, recovery 53.9 / 57.0 / 57.0 s (< 60 s
  budget), both toggle edges observed, `ble.adapter_off` → `ble.adapter_recovered`,
  0 `msg.delivery_failed`.** The 2/3 → 3/3 improvement came from HV-107 (outbound
  dial unblocked) + HV-109 (one persistent GATT server, so the post-toggle
  re-advertise reattaches cleanly). Sub-30 s is a nice-to-have follow-up on
  HV-99, not a blocker. The historical blocker analysis is in
  `hardware_fix_log.md` Session 14.
- **Attempts (3, hardware):** (1) recovered 3/3 toggle cycles (0.8–26 s) but a
  Kotlin-side replay was fragile; (2) STATE_ON replay stalled ~20 s on a
  still-settling stack; (3) core-driven replay — 0/3 in 60 s. Recovery latency
  is dominated by two pre-existing issues outside HV-31's scope: P2 drains the
  adapter events ~20 s late (discovery loop stays in 30 s slow cadence because
  `connections[P1]` is never torn down + nothing calls `wake()`), and P1's
  reconnect `connectGatt` blocks the full 30 s `FfiCallTimeout` (→ HV-99). The
  `btStateReceiver` itself works — both edges observed, GATT server rebuilt,
  core state follows.
- **Progress:** new `btStateReceiver` for `BluetoothAdapter.ACTION_STATE_CHANGED`
  (mirrors the Wi-Fi Direct `p2pStateReceiver`). `STATE_OFF` → close the GATT
  server + clear every stale handle/per-connection map + `drainAdapterEvents()`
  code `2` (core → transport `Unavailable`, drop handles). `STATE_ON` →
  `drainAdapterEvents()` code `3` (core → `Available` + re-drive
  `start_advertising` from a cached `NodeAdvertisement` + force scan re-arm).
  New `BleTransport.last_advertisement` cache; core `discover_peers` performs
  the replay. L1 sim `hv31_bluetooth_toggle_marks_transport_unavailable_then_
  recovers` passes. Hardware attempts 1–2 recovered functionally (3/3 toggle
  cycles, 0.8–26 s) but a Kotlin-side replay stalled ~20 s on a still-settling
  stack — attempt 3 moves the replay into the core (untested). Resume steps in
  `hardware_fix_log.md` Session 14 ("STOPPED HERE").
- **Area:** `AndroidBleTransportAdapter` (no `BluetoothAdapter` state receiver),
  `AndroidWifiDirectTransportAdapter.p2pStateReceiver` (has one),
  `AdapterLifecycle`
- **Severity:** Medium · **HW gate:** 2 phones, toggle airplane mode

**What:** the Wi-Fi Direct adapter listens for `WIFI_P2P_STATE_CHANGED`. The BLE
adapter has **no** `BluetoothAdapter.ACTION_STATE_CHANGED` receiver — if the user
toggles Bluetooth off and on, IRIS's scanner/advertiser/GATT server are all
invalidated at the OS level and nothing re-establishes them; the handles in
`scanHandles`/`advertiseHandles`/`gattServer` are stale. `ensureGattServer()`
early-returns because `gattServer != null`.

**Fix sketch:** add a `BluetoothAdapter` state receiver; on `STATE_OFF` clear all
handles and mark the transport `Unavailable`; on `STATE_ON` rebuild the GATT
server and restart scan/advertise. Mirror what the Wi-Fi Direct
`channelListener`/`p2pStateReceiver` already do.

---

### HV-32 — Two engines, one device: the app + the WorkManager relay worker

- **Fix status:** ✅ Fixed · HW-PENDING · commit 17996d8 · 2026-09-03 · Session
  17 · `IrisBleService.isRunning` (process-local `@Volatile` flag);
  `IrisBackgroundSyncWorker.doWork()` returns `Result.retry()` without touching
  `engine.get()` when the FGS is down — so a bare WorkManager process never
  builds a second `@Singleton IrisEngine`. The relay outbox is durable and
  drains when the service next comes up. `:app` Kotlin + JVM tests green.
  **HW-pending:** the "kill app / trigger worker / confirm no 2nd engine" bench
  run (procedure in `hardware_fix_log.md`). Starting the FGS from an expedited
  worker is a documented follow-up.
- **Was:** ⬜
- **Area:** `IrisBleService`, `IrisBackgroundSyncWorker`, `MeshRepository`,
  `RelayOutbox`, `@Singleton IrisEngine`
- **Severity:** Medium · **HW gate:** 1 phone, background 20+ min

**What:** `IrisBackgroundSyncWorker` (15-min cadence) calls
`repository.drainRelayOutbox()`, which calls `engine.get().sendText(...)`. If the
worker runs while the UI process's engine is also live, both drive the same
`@Singleton` engine — fine. But if the process was killed and WorkManager spins up
a fresh process, `Lazy<IrisEngine>` builds a **new** engine, starts its own tokio
runtime, registers transports, and tries to bring radios up in a background
context with no foreground service — likely failing, and possibly racing the real
app if it restarts. This path has never been observed on a device.

**Fix sketch:** the background worker should only drain the outbox if the
foreground service (and thus the real engine + radios) is alive; otherwise it
should start the FGS or defer. Bench: kill the app with queued relay messages,
wait for the worker, confirm what happens.

---

### HV-33 — GATT `133` and other opaque failure codes: no classification, no mitigation

- **Fix status:** ✅ Fixed · HW-PENDING · commit af265d0 · 2026-09-03 · Session
  17 · `gatt_status_is_transient()` parses the `status=<n>` token the Kotlin
  adapter embeds; `133` (GATT_ERROR), `8` (CONN_TIMEOUT), `62` (FAIL_ESTABLISH),
  `22`/`19` (CONN_TERMINATE_*) → `NotConnected` (retryable + held: HV-7 retries
  the frame, then `close_peer` + holds the message + reconnects — never
  `DeliveryFailed` on one blip); `257` (GATT_FAILURE) + unknown stay `Protocol`.
  Kotlin `onConnectionStateChange(DISCONNECTED)` now fails the in-flight write
  immediately with the status (was: 12 s timeout). L1
  `hv33_transient_gatt_status_codes_are_held_not_failed`. **HW-pending:** 133
  needs connection churn to reproduce; the 30-min sustained session's natural
  blips are the intended evidence. `autoConnect=true`-on-reconnect → **HV-101**.
- **Was:** ⬜
- **Area:** `AndroidBleTransportAdapter` `onConnectionStateChange` /
  `onCharacteristicWrite` status handling, `ble.rs::gatt_status_is_transient`
- **Severity:** Medium · **HW gate:** 2 phones (133 reproduces under connection
  churn)

**What:** `onConnectionStateChange` and `onCharacteristicWrite` capture `status`
into an error string (`"gatt write failed status=$status"`,
`"disconnected before connect completed, status=$status"`). Android GATT status
`133` (`GATT_ERROR`, a catch-all often meaning "transient stack failure, just
retry") and `8` (`GATT_INSUF_AUTHORIZATION` / connection timeout) are lumped in
with genuine failures. `133` on connect is famously fixed by a short delay +
retry, or by calling `connectGatt` from the main thread, or `refreshDeviceCache`.
None of that is done.

**Fix sketch:** classify status codes: `133`/`8`/`22` on connect → bounded retry
with backoff (and try `autoConnect=true` on the 2nd attempt); `257`
(`TOO_MANY_ADVERTISERS`-adjacent) → back off advertising; genuine
`GATT_CONN_TERMINATE_PEER_USER` → don't retry. Document the code→action table.

---

### HV-34 — Bonding/pairing: DECIDED — no OS bond; durable pairing is an app-level known-peer list

- **Fix status:** 🟢 HW-VERIFIED · commit `<pending>` ·
  2026-09-04 · Session 21 · **Decision: IRIS does NOT do OS-level BLE bonding.**
  Research (2026-09-04) killed the main rationale — bonding does **not** speed up
  or harden reconnects: *"Bonding is a security layer. The reconnection delay is
  happening below that, at the connection establishment layer. Re-bonding adds
  latency and user friction for zero gain"*
  ([Android 15 BLE reconnection](https://bleadvertiserapp.medium.com/android-15-broke-ble-reconnection-speed-heres-the-fix-2525bda500e7)).
  What bonding *would* give — RPA-rotation identity resolution (IRK) and
  link-layer encryption — is either already handled (identity comes from the
  **signed beacon `peer_short`** threaded through `known_addresses` /
  `inbound_handles`, HV-13/HV-105/HV-107) or redundant (IRIS does its own E2E
  crypto). Costs avoided: a system pairing dialog, a persistent bond record
  (privacy give-back), an `ACTION_BOND_STATE_CHANGED` state machine, OEM
  bonding flakiness.
- **What was built instead:** a persisted app-level **known-peer list**
  (`iris_bench` snippet: `addFriend` / `knownPeers` / `clearFriends` RPCs, stored
  at `<filesDir>/iris_known_peers.json`; `startMesh` re-feeds every entry into
  the engine key directory). Two devices that have exchanged identities stay
  "friends" across app restart / reinstall — a fresh engine can seal addressed
  mail to them with no re-registration. `registerPeerKey` also persists now, so
  every bench run builds the list up. The real app's contacts UI is HV-56.
- **HW:** `test_known_peers_survive_engine_restart` (`session-21-hv34-fix`) —
  **PASS 3/3**, recoveries 9.5–38.9 s: register P1↔P2 as friends, bounce P2's
  engine 3× with **no** `registerPeerKey`, addressed sends both ways still
  seal + deliver from the persisted key alone.
- **Revisit if:** a truly backgrounded IRIS (no foreground service) becomes a
  requirement — the OS only auto-reconnects a *bonded* device in that state.
- **Area:** `iris_bench` snippet `IrisSnippet`, `AndroidBleTransportAdapter.connectGatt`
  (kept `autoConnect = false`), `PRIVACY_MODEL.md`
- **Severity:** Medium (architectural) · **HW gate:** 2 phones

---

### HV-104 — BLE poller consolidation (+ battery profile — deferred to a rig)

- **Fix status:** 🟢 HW-VERIFIED (part A) · commit `<pending>` ·
  2026-09-04 · Session 21 · **the per-peer inbound pollers are now one
  transport-wide poller.** Was one `tokio::spawn` per connected peer at a fixed
  50 ms cadence (GAP-12): 8 peers = 8 tasks × 20 wakeups/s even fully idle, which
  also kept the SoC out of its deeper sleep states. Now `ensure_accept_poller`
  runs a single task that: (1) on a 500 ms housekeeping beat picks up newly
  `accepted_connections()` and drains `drain_disconnected_handles()`; (2) on a
  fast/idle beat (50 ms if *any* handle produced a frame in the last 2 s, else
  500 ms) calls `drain_inbound_once`, which iterates every live `connections` +
  `inbound_handles` entry, drains each handle, and reassembles into a per-handle
  `Reassembler`. `spawn_inbound_poller` and the `pollers` map are deleted;
  `connect()` just calls `ensure_accept_poller` (idempotent). Same shape the
  Wi-Fi Aware transport already uses for its single poller.
- **HW (`session-21-hv101-104-34`):** `test_pingpong_300char_10x` **PASS 10/10**
  both ways; `test_forced_drop_reconnect_10x` **PASS 10/10** (3.5–14.5 s);
  `test_reconnect_mesh_cycle` **PASS 5/5, 0.3–0.5 s** — *faster* than
  Session 20's 0.4–32.8 s (also carries HV-101's `TRANSPORT_LE`). The
  consolidated poller handles drop/reattach/multi-peer demux exactly as the
  per-peer ones did. iris-core `cargo test` 793/793 (incl. `rt016` frame
  de-mux, `hv98` re-attach, `hv107`, `large_message`, `out_of_order`).
- **Deferred (part B, needs a rig):** the actual 30-min screen-off battery
  measurement + adaptive idle cadence keyed on charge state / battery level.
  Needs 3+ phones (to see the poller cost at scale) + the EXP-003 power rig.
- **Area:** `ble.rs` `ensure_accept_poller` / `drain_inbound_once` (was
  `spawn_inbound_poller` × N), EXP-003 battery rig
- **Severity:** Medium (battery / scale) · **HW gate:** part A done on 2 phones;
  part B needs 3+ phones + a power rig

---

### HV-101 — reconnect-path connect hardening (Android 14/15 direct-connect stall)

- **Fix status:** 🟢 HW-VERIFIED · commit `<pending>` · 2026-09-04 ·
  Session 21 · **the original premise (`autoConnect = true` reconnects faster)
  is wrong** — re-reading the cited source
  ([bleadvertiserapp.medium.com "Android 15 Broke BLE Reconnection Speed"](https://bleadvertiserapp.medium.com/android-15-broke-ble-reconnection-speed-heres-the-fix-2525bda500e7)):
  *"autoConnect = true … is optimised for power, not speed. For foreground
  reconnection where the user is waiting, autoConnect = false is always
  correct."* And *"Re-bonding adds latency and user friction for zero gain"*
  (→ HV-34). The real Android-15 change is stricter GATT connection-cache
  validation that stalls the *first* reconnect 6–8 s; the fix is a state machine
  with **(a) explicit transport, (b) a bounded connect watchdog, (c) exponential
  connect backoff, (d) status-133-is-transient handling.** IRIS already has
  (b) (`connectionReady` + `CONNECT_READY_TIMEOUT_MS = 12 s`, then
  disconnect/close/throw), (c) (`DiscoveryManager::connect_backoff`:
  5→10→20→40→80 s, capped), (d) (HV-33). **This change is (a):** pass
  `BluetoothDevice.TRANSPORT_LE` to `connectGatt` so a dual-mode peer's stack
  never wastes the budget probing BR/EDR first. `autoConnect` stays `false`
  everywhere. No FFI change.
- **Area:** `AndroidBleTransportAdapter.connectGatt`
- **Severity:** Medium · **HW gate:** 2 phones (reconnect/forced-drop suite)

**Superseded fix sketch:** an `auto_connect: bool` on the `connect_gatt` FFI /
`reconnect_gatt`, chosen from whether `known_addresses` /
`connect_backoff` already had this peer. Measure cold-connect vs reconnect
latency bonded/unbonded (folds into HV-34).

---

## Tier 4 — Multi-hop, relay, flood, PRoPHET (never run on hardware)

### HV-35 — Multi-hop relay has never been exercised on physical radios

- **Fix status:** ⬜ · **HW gate:** 🔒 **needs a 3rd phone**
- **Area:** `message_engine/mod.rs` (`deliver_or_relay`, `enqueue_relay`,
  `spawn_delivery_loop`, hop ceiling), `flood.rs`, `engine.rs`
  `spawn_inbox_forwarder`
- **Severity:** Critical (core value prop) · 

**What:** the core *has* a relay path: `process_incoming` → `deliver_or_relay` →
`enqueue_relay` (hop ceiling, per-sender rate-limit and quota on relayed traffic)
→ `spawn_delivery_loop` dequeues and `select_transports` + `send`. It is covered
by `sysval_dtn_multihop` and `sim_scenarios` — **in the simulator**. On hardware:
A→B→C where A and C are not in radio range has never been tried. Unknowns: does B
actually re-advertise / stay discoverable to C while relaying? Does the relayed
envelope keep a valid signature through B (the fragment re-sign path is
sender-only)? Does the hop count increment correctly across the FFI boundary? Does
B's `connections` map hold links to both A and C simultaneously given the BLE
concurrent-GATT limits (HV-18)?

**Fix sketch:** 3-phone bench, A and C separated by distance/walls so they cannot
hear each other, B in the middle. Send A→C. Instrument every hop
(`iris.route.relay` log). Verify delivery, hop count, latency, and that B does not
also deliver the message locally (it's not the recipient).

---

### HV-36 — Flood fan-out is meaningless on a 2-node bench and untested on 3+

- **Fix status:** ⬜ · **HW gate:** 🔒 **needs 3+ phones**
- **Area:** `flood.rs` (`recipients_for_flood`, `MAX_FLOOD_FANOUT = 8`,
  `max_hops_for_priority`), `NeighborTable`, `best_transport`
- **Severity:** High

**What:** `recipients_for_flood` walks `neighbor_table.neighbor_summaries()`,
excludes the sender and recipient, picks `best_transport` per neighbor, caps at 8.
On 2 phones there is never more than one neighbor, so flood == direct and nothing
is tested. On 3+, the questions are: does `NeighborTable` actually get populated
from BLE/Wi-Fi Direct discovery on hardware (it's fed by `DiscoveryManager` — is
that wired to the neighbor table on Android?), does dedup (`ForwardedCache`)
prevent the storm, and does a P0 flood (16 hops, epidemic) behave on real radios
with real loss.

**Fix sketch:** 3–4 phone bench in a line and in a triangle. Broadcast a P4
message from one end; verify every node receives it exactly once and the fan-out
cap holds. Then a P0. Measure duplicate-delivery rate and airtime.

---

### HV-37 — PRoPHET delivery predictability has never touched a real mobility trace

- **Fix status:** ⬜ · **HW gate:** 🔒 **needs 3+ phones + movement**
- **Area:** `routing/prophet.rs`, `routing/scf.rs`, `routing/opportunistic.rs`
- **Severity:** High

**What:** PRoPHET and store-carry-forward are simulator-validated only
(`sim_scenarios`). Their whole premise — encounter history predicts future
delivery — requires actual node mobility. On the bench this needs someone to walk
a phone between two stationary phones repeatedly while messages queue.

**Fix sketch:** define a repeatable "courier" bench: phone C carried between
rooms A and B on a fixed loop; messages queued A→B while they're partitioned;
verify C picks them up, carries, and delivers, and that the delivery-predictability
metric moves in the right direction. This is a multi-session effort.

---

### HV-38 — The Android relay outbox (`RelayOutbox`) is only the local sender's retry queue, not mesh relay

- **Fix status:** ⬜
- **Area:** `RelayOutbox`, `MeshRepository.send` / `drainRelayOutbox`,
  `IrisBackgroundSyncWorker`
- **Severity:** Medium (conceptual clarity + a real gap) · **HW gate:** 2 phones

**What:** `RelayOutbox` holds messages *this node originated* that failed
`engine.sendText` (FFI threw), drained every 15 min by WorkManager or on `/relay`.
It is **not** the core's relay queue (`enqueue_relay` in the message engine, for
forwarding *other people's* messages). The UI's "Q3" chip
(`state.relayQueued`) reflects only the local outbox. So the operator's mental
model ("relay/flood/prophet never tested") is partly a naming problem — the core
relay is untested on HW (HV-35), and the Android outbox is a *different* thing
that papers over transient FFI failures with a 15-minute latency floor.

**Fix sketch:** rename to `PendingSendQueue`; surface the core's actual relay
metrics (`metrics.relayed`, relay queue depth) separately via `snapshot()` (HV-3);
shorten the WorkManager cadence or make it event-driven (drain on next transport
`Available`).

---

### HV-39 — Relayed envelope integrity across a hop: signature, hop_count, TTL

- **Fix status:** ⬜ · **HW gate:** 🔒 needs 3 phones (or a careful 2-phone proxy)
- **Area:** `message_engine/crypto.rs`, `message.rs` (`Envelope` hop fields),
  `enqueue_relay`, `deliver_outbound` (re-encode)
- **Severity:** High

**What:** when B relays A's message, B `codec::encode`s the envelope it received
and sends it on. The signature covers a defined signing scope; `hop_count` is
incremented somewhere on the relay path (need to confirm where — `enqueue_relay`
enforces a ceiling, but does it bump the count, and is the count inside or outside
the signed scope?). If `hop_count` is signed, B cannot increment it without
breaking A's signature; if it's unsigned, an attacker can reset it to dodge the
hop ceiling (this is called out in a `mod.rs` comment about the replay high-water
and a "relay-incremented field"). This must be pinned down and tested on a real
hop.

**Fix sketch:** trace the signing scope vs the mutable routing fields; write a
2-node test where node B is instrumented to dump the exact bytes it forwards vs
what it received; confirm C accepts and the hop accounting is both correct and
not attacker-malleable.

---

### HV-40 — No topology visibility: the operator cannot see the mesh graph on device

- **Fix status:** ⬜
- **Area:** `NeighborTable`, `observability`, a `/mesh` or `/peers` command,
  `MeshUiState`
- **Severity:** Medium · **HW gate:** 3 phones

**What:** `/peers` returns a `LINKS` block with only counts. `MeshUiState` has no
neighbour list. There is no way, on a phone, to see: who are my direct neighbours,
over which transport, at what link quality, and what's my 2-hop horizon. Debugging
"the mesh never widens past one peer" is impossible without this.

**Fix sketch:** expose `NeighborTable` summaries over FFI; render `/peers` as a
real list (peer short-id, transport, RSSI/quality, last-seen, state); add the
discovered-but-not-connected set too so "found but can't connect" is visible.

---

### HV-41 — Broadcast / group messaging is not exposed on Android at all

- **Fix status:** ⬜
- **Area:** `engine.rs` `send_text` (always addressed to a recipient),
  `build_text_envelope`, `deliver_or_relay` (has a broadcast branch),
  `CommandExecutor`
- **Severity:** High (product) · **HW gate:** 3 phones

**What:** the core `deliver_or_relay` clearly supports broadcast
(`recipient_id`-less / broadcast-group envelopes, emergency broadcast). But
`IrisEngine::send_text` always builds an envelope with a concrete `recipient_id`,
and `CommandExecutor` always requires a recipient. There is no "send to everyone
nearby" / channel / group primitive in the Android app — so flood is never even
requested by the shell, only relay-toward-a-recipient.

**Fix sketch:** add an FFI `broadcast_text(text, priority)` that builds a
broadcast envelope; add a `/all <msg>` or a "Broadcast" recipient mode in the
shell; this is also what makes HV-36 (flood on hardware) actually reachable from
the UI.

---

## Tier 5 — Internet / TCP-IP transport (absent on Android)

### HV-42 — There is no internet transport wired into the Android engine

- **Fix status:** ⬜
- **Area:** `crates/iris-android/src/engine.rs` (registers only `BleTransport`,
  `WifiAwareTransport`, `WifiDirectTransport`), `crates/iris-core/src/transport/internet.rs`
  (exists, not used on Android), `crates/iris-android/src/ffi/` (no internet adapter),
  `crates/iris-android/src/bridge.rs`
- **Severity:** Critical (operator's explicit complaint) · **HW gate:** 2 phones,
  both on internet

**What:** `grep -i 'internet\|gateway\|tcp' crates/iris-android` returns nothing.
The Android `IrisEngine::new` registers exactly three transports. `internet.rs`
provides `encode_frame`/`frame_payload_len`/`FRAME_HEADER_LEN` (used by
`wifi_direct.rs` for framing) and, per `docs/implementation/INTERNET_TRANSPORT_VERIFICATION.md`,
a TCP transport with connection pooling and backoff — but it is a core module with
no Android FFI adapter and no registration. So the operator's "with internet the
message is not being sent, ever" is correct: **IRIS on Android has no internet
path whatsoever.** Two phones on the same LAN or both with mobile data cannot use
TCP/IP to exchange a message.

**Note — partly a deliberate v1 scope cut.** The manifest comment says: *"targetSdk
34 … the SDK-37 `ACCESS_LOCAL_NETWORK` cliff (data plane = TCP/UDP/mDNS/DNS-SD) is
deliberately avoided for v1. See ANDROID_DESIGN.md §8."* LAN peer-to-peer was
consciously deferred. But an **outbound client→gateway/relay** path does not hit
that cliff and is not covered by that decision — re-evaluate shipping at least
that in v1, and treat LAN P2P (HV-43) as the deferred piece.

**Fix sketch (this is a build-out, not a bug fix):**
1. Decide the model: direct peer-to-peer TCP (needs one side reachable — NAT
   traversal / a rendezvous), or a gateway/relay server (the `gateway/` module +
   `docs/routing/GATEWAY_SELECTION.md`), or both.
2. If direct: an Android `InternetTransport` doesn't need a platform adapter the
   way BLE does — it can run entirely in Rust (tokio TCP) since Android grants
   `INTERNET` freely. Register `InternetTransport` in `engine.rs` guarded by
   network availability (`ConnectivityManager` via a thin FFI callback).
3. Addressing: a peer's reachable `SocketAddr` has to come from somewhere — the
   `NodeAdvertisement.public_ip_addr` field exists and is always `None` today.
   Needs a discovery mechanism (mDNS on LAN, a signalling server, or manual
   `/to <peerid>@<ip:port>`).
4. Wire it into `select_transports` with the right cost class (metered vs Wi-Fi).

**HW verification:** two phones on the same Wi-Fi LAN exchange a message with
Bluetooth and Wi-Fi Direct both off. Then two phones on mobile data (via whatever
rendezvous is chosen).

---

### HV-43 — LAN peer discovery (mDNS / NSD) does not exist

- **Fix status:** ⬜ · depends on HV-42
- **Area:** new — Android `NsdManager` or Rust mDNS, `DiscoveryManager`
- **Severity:** High (makes HV-42 usable) · **HW gate:** 2 phones on one LAN

**What:** even with an internet transport, two phones on the same Wi-Fi have no way
to find each other's IP:port. Wi-Fi Direct's DNS-SD is P2P-only. Standard mDNS/NSD
(`_iris._tcp` on the infrastructure network) is the natural fit and is not
implemented.

**Fix sketch:** Android `NsdManager` registration + discovery for `_iris._tcp`
carrying the 22-byte beacon in a TXT record (same beacon codec as Wi-Fi Direct);
feed matches into the neighbour table with an `internet` transport address.

---

### HV-44 — No NAT traversal / rendezvous for phones not on the same network

- **Fix status:** ⬜ · **HW gate:** 🔒 needs a server + 2 phones on different networks
- **Area:** `gateway/`, `docs/routing/GATEWAY_SELECTION.md`,
  `docs/transports/INTERNET.md`
- **Severity:** High (this is what "internet messaging" means to most users)

**What:** two phones on different mobile networks are both behind carrier-grade
NAT. Direct TCP is impossible without a relay or hole-punching + a signalling
server. IRIS's design mentions gateways; nothing runs.

**Fix sketch (research + build):** evaluate a minimal relay server (store-and-
forward for offline peers, live relay for online ones) vs a STUN/TURN-style
approach. This is a significant piece of infrastructure and a separate project
node; scope it, don't rush it.

---

### HV-45 — `ConnectivityManager` signal is not consumed — the app doesn't know it's online

- **Fix status:** ⬜ · depends on HV-42
- **Area:** new FFI callback for network state, `engine.rs`, `IrisApplication`
- **Severity:** Medium

**What:** nothing in the Android layer observes `ConnectivityManager` /
`NetworkCallback`. The engine cannot bring an internet transport up when Wi-Fi
connects, or tear it down / mark `Unavailable` when the network drops, or
distinguish metered from unmetered for cost-based selection.

**Fix sketch:** a `NetworkCallback` in the FGS that pushes
online/offline/metered/unmetered over FFI to the (future) `InternetTransport`.

---

## Tier 6 — Transport selection & concurrent-radio coexistence

### HV-46 — BLE and Wi-Fi (Aware/Direct) share the 2.4 GHz radio; the conflict-group logic only dedupes Wi-Fi-vs-Wi-Fi

- **Fix status:** ⬜
- **Area:** `transport/manager.rs` `select_transports` (RadioConflictGroup),
  `ble.rs` (`Bluetooth24GHz`), `wifi_direct.rs` / `wifiaware.rs` (`WiFi24GHz`)
- **Severity:** High · **HW gate:** 2 phones, force multipath (P0)

**What:** `select_transports` enforces "at most one transport per non-None conflict
group." BLE is `Bluetooth24GHz`; Wi-Fi Aware and Wi-Fi Direct are `WiFi24GHz`. So
the logic will pick *one* of {Wi-Fi Aware, Wi-Fi Direct} plus BLE — it treats
Bluetooth and Wi-Fi as non-conflicting. Physically, BLE and 2.4 GHz Wi-Fi share
the band and the antenna on most phones; running an active BLE GATT transfer and a
Wi-Fi Direct socket transfer simultaneously (which P0 multipath explicitly does)
causes mutual interference, coexistence-driven throughput collapse, and on some
chipsets one radio starving the other. This has never been measured.

**Fix sketch:** measure BLE + Wi-Fi Direct concurrent throughput on the bench
phones. If it's bad, either (a) put BLE and 2.4 GHz Wi-Fi in a soft conflict group
(prefer not to run both hot at once, but allow it for P0), or (b) coordinate:
control plane on BLE, bulk on Wi-Fi, never both saturated. Document the AFH /
coex reality per chipset seen.

---

### HV-47 — Multipath P0 sends the same message over multiple transports with no dedup guarantee at the receiver-transport layer

- **Fix status:** ⬜
- **Area:** `deliver_outbound` (`multipath: priority.is_emergency()`),
  `message_engine` dedup (`ForwardedCache`, message-id dedup)
- **Severity:** Medium · **HW gate:** 2 phones

**What:** a P0 goes over BLE *and* Wi-Fi Direct simultaneously. The receiver gets
two copies; message-id dedup in `process_incoming` should drop the second
(`InboundOutcome::Duplicate`). But the fragmentation layer assigns per-fragment
derived ids, and the two transports may fragment differently (different MTUs), so
the dedup has to happen on the *reassembled* original id, not the fragment id.
Needs an explicit hardware test that a P0 delivered twice surfaces once.

**Fix sketch:** bench a P0 with both transports up; confirm exactly one delivery
to the app and one `Duplicate` in the log; confirm no wasted full reassembly of
the loser.

---

### HV-48 — `select_transports` runs on `TransportState`, which lags reality by a poll interval or more

- **Fix status:** ✅ Fixed · commit 4a2889b · 2026-09-04 · Session 18 · §5 group
  with HV-27 · `score_transport` now applies `-15` (Poor) / `-5` (Fair) to a
  transport whose `link_quality(req.target_peer)` (HV-27) has gone quiet —
  bounded under the 20-pt `state()` gap so it re-ranks *between* transports for
  the same peer without demoting a Connected transport below an Available one on
  its own. L1 `hv48_selection_prefers_the_transport_with_a_healthy_link_to_the_peer`.
  Verified end-to-end only when a 2nd transport is up (Tier 6); the L1 + HV-27's
  bench check cover it for now.
- **Was:** ⬜
- **Area:** `manager.rs` `score_transport`, `Transport::link_quality`
- **Severity:** Medium · **HW gate:** 2 phones

**What:** selection filters on `state()` ∈ {Available, Degraded, Connected}.
`Connected` for BLE is set when `connections` is non-empty; it falls back to
`Available` in `close_peer` only when the map empties. A link that is silently
dead (HV-27) keeps the transport `Connected`, so it keeps getting selected and
every send fails and requeues. There's a `HW-4` log for "no transport selected"
but not for "selected a transport whose link is dead."

**Fix sketch:** depends on HV-27 (per-link health). Selection should consider "do
I have a *healthy* link to this specific peer over this transport," not just the
transport's coarse state.

---

### HV-49 — Requeue/backoff on send failure: verify a message doesn't spin forever or expire silently

- **Fix status:** ⬜
- **Area:** `message_engine/mod.rs` `requeue_or_fail`, `deliver_outbound`,
  `queue.rs` (`QueuedMessage.attempts`), `expiry`
- **Severity:** Medium · **HW gate:** 2 phones

**What:** on `select_transports` empty or `send` failure, `deliver_outbound` calls
`requeue_or_fail`. The message re-enters the queue and is retried every
`poll_interval`. If the peer is genuinely unreachable, the message loops until
`ttl_seconds` (3600 by default from `build_text_envelope`) then expires — the user
sent something an hour ago, it never went, and (per HW-2) it shows as "sent" in
the UI the whole time with no failure surfaced. `requeue_or_fail` has a `bool`
that presumably distinguishes transient from permanent, but the UI never learns.

**Fix sketch:** cap retry attempts / add a shorter "no route" TTL for interactive
messages; surface `Expired` / `Failed` to the inbox listener so the UI can show a
message as failed (strike-through, retry button); the `/relay` outbox already has
a "pending" state — reuse it.

---

### HV-50 — Wi-Fi Aware vs Wi-Fi Direct: only one `WiFi24GHz` transport is ever selected, but both are started

- **Fix status:** ⬜
- **Area:** `engine.rs` `start_all` (starts all three), `manager.rs` conflict group
- **Severity:** Low–Medium · **HW gate:** 2 phones

**What:** `start_all` brings up BLE + Wi-Fi Aware + Wi-Fi Direct. Selection then
only ever uses one of the two Wi-Fi transports. So one of Aware/Direct is always
running its discovery, publish/advertise, and pollers for nothing — battery and
radio time spent on a transport that will never be selected while the other is
`Connected`. Also both compete for the same 2.4 GHz Wi-Fi resource.

**Fix sketch:** pick one Wi-Fi transport as primary (Aware where the hardware
supports it — it's lighter-weight and needs no group; Direct as fallback) and
keep the other cold until the primary goes `Unavailable`. Don't run both
discovery loops simultaneously.

---

## Tier 7 — Wi-Fi Aware data path

### HV-51 — Wi-Fi Aware NDP responder — the largest known unfinished transport item (FFI-6)

- **Fix status:** ⬜ · **HW gate:** 🔒 2 phones *with NAN hardware*
- **Area:** `crates/iris-core/src/transport/wifiaware.rs`,
  `crates/iris-android/src/ffi/wifi_aware_adapter.rs`,
  `AndroidWifiAwareTransportAdapter.kt`
- **Severity:** High (but lower priority than BLE/Direct which do work)

**What:** the prior hunt's own summary (`README.md` Tier 2–3 row) lists "FFI-6
(Wi-Fi Aware NDP responder — largest remaining architectural item)" as still open.
`AndroidWifiAwareTransportAdapter.kt` had 26 compile errors originally
(ANDROID_BUILD_STATUS.md — the most of any file). The NDP (Network Data Path)
responder half — the side that accepts an incoming data-path request — is not
fully implemented. Wi-Fi Aware also requires specific hardware; one of the current
bench phones may not have it (`start_all` already tolerates "wifi-aware-0 failed:
not supported").

**Fix sketch:** this is a build-out. Given BLE and Wi-Fi Direct are the working
transports and Wi-Fi Aware is hardware-gated, **defer this behind Tiers 1–6**
unless both bench phones have NAN and the operator wants it. Document the
NDP initiator/responder contract, implement the responder, test on NAN hardware.

---

### HV-52 — `start_all` treats "Wi-Fi Aware not supported" as normal but still runs its poller

- **Fix status:** ⬜
- **Area:** `engine.rs` `start_all`, `wifiaware.rs` state,
  `spawn_inbox_forwarder`
- **Severity:** Low · **HW gate:** 1 phone without NAN

**What:** `start_all` correctly continues if `wifi-aware-0` fails with "not
supported." But the transport is still registered, its inbox forwarder task is
still spawned, and its poller may still run against an adapter that always returns
empty / `is_available() == false`. Minor wasted work; also clutters `/diag`.

**Fix sketch:** if a transport reports permanently `Unavailable` / `NotSupported`
at `start_advertising`, deregister it (or mark it dormant and skip its poller)
rather than leaving a dead transport in the manager.

---

### HV-53 — Wi-Fi Aware and Wi-Fi Direct both advertise `_iris` services simultaneously — cross-talk

- **Fix status:** ⬜ · **HW gate:** 🔒 2 phones with NAN
- **Area:** `wifiaware.rs` publish, `wifi_direct.rs` DNS-SD, service naming
- **Severity:** Low–Medium

**What:** if both Wi-Fi transports are up (HV-50), a phone publishes an IRIS Wi-Fi
Aware service *and* an IRIS Wi-Fi Direct DNS-SD service. A peer may discover the
same physical node twice over two Wi-Fi transports and, given the candidate-id
keying, potentially create two neighbour entries. Interacts with HV-13/FFI-1.

**Fix sketch:** the neighbour table should reconcile a node discovered over
multiple transports into one entry with multiple transport addresses (it may
already — needs verification on hardware).

---

## Tier 8 — Shell UX

### HV-54 — The composer covers the newest messages; multi-line input makes it worse

- **Fix status:** 🟢 HW-verified · commit `<pending>` · 2026-09-05 · Session 22 ·
  P1 (vivo V2205, Android 15). `ConsoleScreen`'s floating composer/palette
  column now measures its own height via `onGloballyPositioned` and feeds it
  (+ a small gap) into the transcript `LazyColumn`'s bottom `contentPadding`,
  replacing the old static `IrisSizing.InputHeight + IrisSpacing.XXL`; a
  `LaunchedEffect(floatingHeight)` re-scrolls to the tail whenever the
  composer's measured height changes (not just when a new entry arrives), so
  the last message can't slide back under a growing composer. HW-verified:
  installed on P1, typed a 1→3→4-line message via `adb shell input text` +
  `KEYCODE_ENTER` and screenshotted at each stage — the composer visibly
  grows from 1 to 4 lines and the reserved transcript padding grows with it
  (screenshots show no content clipped behind the composer at any stage).
- **Area:** `ConsoleScreen.kt` — `LazyColumn` `contentPadding` bottom is the
  **static** `IrisSizing.InputHeight + IrisSpacing.XXL`; the input `Column` floats
  over it and grows upward to `maxLines = 5`
- **Severity:** High (operator's explicit complaint) · **HW gate:** any phone

**What:** the transcript reserves a fixed bottom padding sized for a one-line
composer. `IrisConsoleInput` is `heightIn(min = InputHeight)` with
`BasicTextField(singleLine = false, maxLines = 5)`, so as the user types a longer
message the composer expands **upward**, over the last rendered messages, and the
list's reserved padding does not grow to match. Result: you cannot see the last
1–3 messages while composing, and on a 2–5 line message you cannot see the top of
what you are typing either (the field scrolls internally but the row is clipped by
the glass surface / screen edge).

**Fix sketch:** measure the composer's actual height (`onGloballyPositioned` or
`SubcomposeLayout`) and feed it into the `LazyColumn`'s `contentPadding` bottom so
the transcript always reserves exactly the composer's current height + a gap; when
the composer grows, the list scrolls to keep the tail visible. Cap the composer at
~4 lines with internal scroll and a clear visual boundary.

---

### HV-55 — There is no send button

- **Fix status:** 🟢 HW-verified · commit `<pending>` · 2026-09-05 · Session 22 ·
  P1 (vivo V2205, Android 15). Added an explicit `Icons.AutoMirrored.Filled.Send`
  `IconButton` in `IrisConsoleInput`'s `Row`, right of the text field, enabled
  only when `value.isNotBlank()` and tinted `AccentPrimary`/`TextQuaternary`
  to make the enabled state visible; calls the same `onSubmit` the IME action
  calls, with a `contentDescription = "Send"` semantics node for TalkBack.
  IME `ImeAction.Send` stays as a shortcut, unchanged. HW-verified: tapped
  the button on P1 with an empty recipient set — it fired the same
  `onSubmit` path as the keyboard's Send key (surfaced the expected "No
  recipient" system error, proving it's wired through, not a no-op); tapped
  it again after typing a message and setting a recipient — the input
  cleared and the composer collapsed back to placeholder height, matching
  the keyboard-Send behavior exactly.
- **Area:** `IrisConsoleInput.kt` (relies solely on `ImeAction.Send` /
  `KeyboardActions(onSend)`)
- **Severity:** High (operator's explicit complaint) · **HW gate:** any phone

**What:** the only way to send is the soft keyboard's "Send" action key. Many
keyboards (Gboard with certain languages, third-party keyboards, hardware
keyboards, some RTL layouts) render that key as a newline or "done" and the
`onSend` never fires — the user types a message and nothing happens on Enter, with
no visible affordance to send. There is also no send button for accessibility
(TalkBack users, motor-impaired users) and no way to send without dismissing to
find the right keyboard key.

**Fix sketch:** add an explicit send/`▷` icon button in the composer `Row` (right
of the text field), enabled when `value.isNotBlank()`, calling the same
`onSubmit`. Keep the IME action as a shortcut. For SOS/P0 consider a distinct
affordance.

---

### HV-56 — No contacts / peer list / nicknames — every message requires pasting a 64-hex PeerId

- **Fix status:** ⬜
- **Area:** `CommandExecutor.setRecipient` (requires exactly 64 hex),
  `MeshViewModel._recipient` (a raw hex string), no contact store anywhere,
  `MeshUiState` (no peer list)
- **Severity:** High (operator's explicit complaint) · **HW gate:** 2 phones

**What:** to send anything you must run `/to <64-hex>` or `@<64-hex>`. There is no
persistent contact store, no nickname mapping, no "recently discovered peers"
picker, no way to save a peer you've messaged. `/node` shows your own full hex to
read aloud/copy to the other person. This is unusable beyond a one-off demo.

**Fix sketch:**
1. A `ContactStore` (Room / DataStore): PeerId ↔ user-assigned name, added-on,
   last-seen, favourite.
2. Surface discovered peers (from `NeighborTable` via FFI — see HV-40) as a
   pickable list; tapping one sets the recipient and offers "save as contact."
3. `@name` / `/to name` resolves through the contact store; fall back to hex.
4. An exchange mechanism: show a QR code of your PeerId (`/node` → QR), scan to
   add a contact. Or NFC tap. Or "peers near me" auto-populated from discovery.
5. The status line's `→ abcd1234…` should show the contact name when known.

---

### HV-57 — Received messages have no reply affordance

- **Fix status:** ✅ Fixed · HW-adjacent · commit `f3d21c2` · 2026-09-05 ·
  Session 23. `IrisMessage` gained an optional `onReply` callback — tapping
  a received message row calls `MeshViewModel.replyTo(senderId)`, which sets
  `_recipient.value` exactly like the already-hardware-verified `/to
  <peerId>` command path (same underlying assignment, same
  `ConsoleEntry.system("RECIPIENT", ...)` echo, just a different trigger).
  Deliberately minimal — no `ContactStore`, no name resolution — HV-56 stays
  open for that. **Not independently HW-tested via an actual tap gesture**
  this session (P2→P1 delivery and the `/to`-then-send path were exercised
  live on hardware while diagnosing a coordinate-mapping issue in the test
  tooling, but the tap-to-reply gesture itself wasn't separately exercised);
  the code path it delegates to is the same one already proven working.
- **Area:** `ConsoleScreen.ConsoleRow` / `IrisMessage`, `MeshViewModel.submit`
- **Severity:** High (drives the "one-way only" perception) · **HW gate:** 2 phones

**What:** when a message arrives, its sender id is in `InboxUiMessage`, but tapping
the message does nothing. To reply you must copy the sender's PeerId (not shown in
full) and `/to` it. So in practice conversation is one-directional unless both
users manually exchange hex up front. The operator's "only one-way communication,
unicast only" is largely this + HV-56 + HV-41.

**Fix sketch:** tap a received message → set it as the current recipient (and
offer "save as contact"); show a subtle "replying to <name/shortid>" state in the
composer; long-press for a context menu (reply, copy id, save contact, priority).

---

### HV-58 — No delivery / send status on messages — a failed send looks identical to a delivered one

- **Fix status:** ⬜
- **Area:** `MeshRepository.send` (HW-2 adds the message to the list on FFI
  acceptance, not on delivery), `InboxUiMessage.sent` / `.pending`, no ACK surface
- **Severity:** High · **HW gate:** 2 phones

**What:** `engine.sendText` returning `Ok` means "the envelope was accepted into
the outbound queue," not "it was transmitted" and definitely not "it was
delivered." The UI shows it as a sent message immediately. If the transport never
finds a route (HV-49) the message silently expires an hour later with the UI still
showing it as sent. The core has an ACK path (`message_engine/ack.rs`) — it is not
surfaced to Android.

**Fix sketch:** three states minimum — queued (clock icon), transmitted (single
tick), delivered/acked (double tick), failed/expired (red). Wire
`message_engine/ack.rs` + queue status through the FFI inbox/`snapshot` so the UI
can update a message's state. `InboxUiMessage` already has `sent`/`pending`
variants — extend and drive them from real events.

---

### HV-59 — `MeshStatus` LINK/INIT/IDLE/DOWN is a single global state — it hides per-transport reality

- **Fix status:** ⬜
- **Area:** `MeshUiState.status`, `MeshRepository.startMesh` (sets RUNNING on
  `startAll` returning, which now succeeds if *any* transport started),
  `ConsoleScreen.StatusLine`
- **Severity:** Medium · **HW gate:** 2 phones

**What:** `startMesh` sets `RUNNING` when `engine.startAll()` returns — and
`start_all` now returns `Ok` if *at least one* transport started (correct, per its
comment). So "LINK" can mean "all three radios up and connected" or "BLE only, and
not connected to anyone." The user has no idea whether they can actually reach a
peer right now.

**Fix sketch:** the status line should show per-transport state
(`BLE ● WD ○ NET –`) and, more importantly, "connected to N peers" — RUNNING with
0 peers is not the same as RUNNING with a live link. Feed from `snapshot()`.

---

### HV-60 — `reconnectMesh` / RETRY is the user's main recovery tool and it may not work (see HV-29)

- **Fix status:** ⬜ · duplicate-of-concern with HV-29 but tracked in the UX tier
  because the *button* is the issue
- **Area:** `ConsoleScreen.RetryNotice`, `MeshViewModel.reconnectMesh`
- **Severity:** Medium · **HW gate:** 2 phones

**What:** the UNAVAILABLE state shows a "RETRY" affordance calling
`reconnectMesh()`. If HV-29 is real (engine not restartable), the button appears
to do something (status flips to STARTING then RUNNING) while the mesh stays dead.
Worse UX than no button.

**Fix sketch:** fix HV-29; make the button show real progress and fall back to
"restart the app" guidance if recovery genuinely fails.

---

### HV-61 — Permission UX: the app can sit in a "RUNNING but carries no traffic" state

- **Fix status:** ⬜
- **Area:** `MeshViewModel.init` / `ensureStarted`, `ConsoleScreen`
  PermissionNotice, `MeshPermissions`
- **Severity:** Medium · **HW gate:** fresh install on 2 phones

**What:** `ensureStarted` only runs the engine bring-up if `MeshPermissions.allGranted`.
If the user denies a permission, the notice shows — but the mesh status may still
read INIT/IDLE rather than clearly "blocked on permissions," and if permissions
are partially granted (e.g. `BLUETOOTH_CONNECT` but not `BLUETOOTH_SCAN`) the
behaviour is untested.

**Fix sketch:** a distinct "PERMISSIONS REQUIRED" status; test every partial-grant
combination; on Android 12+ handle the "don't ask again" path with a deep link to
app settings.

---

### HV-62 — No onboarding: first launch drops the user into a terminal with no peer and no guidance

- **Fix status:** ⬜
- **Area:** `ConsoleScreen`, first-run state
- **Severity:** Medium (adoption) · **HW gate:** fresh install

**What:** a new user sees `IRIS <shortid>` and an empty transcript with a
"Message, /command, or @peer" hint. There is no "here is your ID, here is how to
add someone, tap here to scan a QR" flow. The `/help` command exists but the user
doesn't know to type it.

**Fix sketch:** a first-run system message in the transcript: your id (+ QR),
"add a peer with /to or by scanning their code," "type /help for commands." Tie
into HV-56.

---

### HV-63 — The transcript auto-scrolls on every entry, fighting a user who scrolled up to read history

- **Fix status:** ⬜
- **Area:** `ConsoleScreen` `LaunchedEffect(entries.size) { listState.animateScrollToItem(lastIndex) }`
- **Severity:** Low–Medium · **HW gate:** any phone
- **What:** every new message or system event force-scrolls to the bottom, even if
  the user deliberately scrolled up. During an active mesh with discovery/system
  chatter this makes reading older messages impossible.
- **Fix sketch:** only auto-scroll if the user is already near the bottom
  (`listState.layoutInfo`); otherwise show a "N new ↓" pill.

---

### HV-64 — SOS / P0 has no distinct entry path or confirmation on Android

- **Fix status:** ⬜
- **Area:** `CommandExecutor` (`/sos <text>` → `PRIORITY_SOS`, still requires a
  recipient), `build_text_envelope` (P0 → `ContentType::Sos`)
- **Severity:** Medium (safety-adjacent) · **HW gate:** 2 phones

**What:** SOS is `/sos <message>` and **still requires a preset recipient**
(`sendOrReject(text, PRIORITY_SOS, hasRecipient)`) — but an SOS should broadcast to
everyone in range, not go to one hand-entered PeerId. There is no big-red-button
affordance, no confirmation, no location attach (HV in `docs/emergency/`), and if
you haven't done `/to` first it just errors.

**Fix sketch:** SOS should be a broadcast (needs HV-41), reachable without a
recipient, with a deliberate confirm (hold-to-send), and should attach location if
granted. Coordinate with `docs/emergency/SOS.md` and the deferred PRY-11.

---

## Tier 9 — Additional findings from the methodology / internet-research pass

*Added 2026-08-31 after researching the Android BLE/Wi-Fi engineering literature
(Punch Through, Martijn van Welie's "Making Android BLE work", Nordic /
RxAndroidBle / blessed-android issue trackers, AOSP `ScanManager` /
`WifiP2pServiceImpl`, and the Mobly test framework). Each carries its home tier in
brackets — file it there when working the loop; it is grouped here only by
provenance. Sources are listed per finding.*

### HV-65 — GATT writes use `WRITE_TYPE_DEFAULT` (write-with-response); every fragment pays an ACK round-trip

- **Fix status:** ⬜ · **Home tier:** 1 · groups with HV-7
- **Area:** `AndroidBleTransportAdapter.gattWrite` (lines ~634, ~640 —
  `BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT`), characteristic declared with
  `PROPERTY_WRITE | PROPERTY_WRITE_NO_RESPONSE | PROPERTY_NOTIFY` (line ~435)
- **Severity:** High · **HW gate:** 2 phones + `btsnoop`

**What:** the characteristic advertises `WRITE_NO_RESPONSE`, but `gattWrite`
always writes with `WRITE_TYPE_DEFAULT` = write-*with*-response. Write-with-
response requires a full ATT request/response round-trip per fragment; write
commands (no response) are documented at **4–6× the throughput** and are the
standard choice for bulk/chunked transfer. Combined with HV-7 (many fragments)
and HW-7 (each write blocks on `onCharacteristicWrite`), a multi-fragment message
is as slow as BLE gets.

**Nuance from research:** on Android you must *still* wait for the write callback
even for `WRITE_TYPE_NO_RESPONSE` — the stack defers the callback when its buffers
are full, and that is your flow control. So the fix is not "fire and forget," it
is "switch to `WRITE_TYPE_NO_RESPONSE`, keep waiting for the callback, and add an
app-level ACK/window (e.g. every Nth fragment, or a final checksum frame) since
write-without-response can silently drop packets."

**Fix sketch:** use `WRITE_TYPE_NO_RESPONSE` for data fragments; keep the
completion wait; add a lightweight app-level reliability layer (sequence numbers
already exist in `AttSegmenter`; add a NACK/retransmit-request path in the
`Reassembler` → a control write back on `IRIS_IDENTIFY_CHARACTERISTIC` or a notify).
Measure throughput both ways on the bench.

**Sources:** [Punch Through — Write Requests vs Write Commands](https://punchthrough.com/ble-write-requests-vs-write-commands/),
[Punch Through — Android BLE guide](https://punchthrough.com/android-ble-guide/),
[RxAndroidBle #776](https://github.com/dariuszseweryn/RxAndroidBle/issues/776).

---

### HV-66 — No large MTU is requested on connect; `setMtu` is fire-and-forget; the first message goes out at 23 bytes

- **Fix status:** ⬜ · **Home tier:** 1 · groups with HV-7 / HV-8
- **Area:** `AndroidBleTransportAdapter.setMtu` (calls `gatt.requestMtu(mtu)` then
  returns `negotiatedMtu[gatt] ?: mtu` **without waiting** for `onMtuChanged`),
  `connectGatt` / `connectionReady` (resolves on service discovery, not on MTU),
  `ble.rs` (only calls `set_mtu` lazily from the send path)
- **Severity:** High · **HW gate:** 2 phones + `btsnoop`

**What:** research consensus: *"anything sent during the window between connection
and the MTU response goes out at 23 bytes regardless of what gets negotiated —
gate bulk transfer on `onMtuChanged`."* IRIS never proactively requests a big MTU
on connect; `setMtu` is called lazily by the core send path, does not block on
`onMtuChanged`, and returns the requested value optimistically. So the first
message (often the only one the user cares about) fragments at MTU 23. Also
documented: **on some Samsung Android 9/10 devices `requestMtu` immediately after
connect fails silently — a ~600 ms delay before the request is needed.**

**Fix sketch:** in the central role, after service discovery, call
`requestMtu(517)` (with a short post-connect delay), and do **not** resolve
`connectionReady` / report the transport usable until `onMtuChanged` has fired or
a ~1 s timeout elapses. Feed the negotiated value into the Rust `connections` map.
See HV-8 for the peripheral-role half.

**Sources:** [Hubble — BLE MTU Negotiation](https://hubble.com/community/guides/ble-mtu-negotiation-explained-how-to-send-more-data-per-packet/),
[uynguyen — Reliable BLE Data Transfer](https://uynguyen.github.io/2026/04/12/Reliable-BLE-Data-Transfer-MTU-Throughput-Chunking/),
[Punch Through — Maximizing BLE Throughput Pt 4](https://punchthrough.com/ble-throughput-part-4/).

---

### HV-67 — No per-connection GATT operation queue — connect / discover / requestMtu / write race across threads

- **Fix status:** ⬜ · **Home tier:** 1 (cross-cutting)
- **Area:** `AndroidBleTransportAdapter` — `writeCompletion` serializes *writes*
  only; `discoverServices` is fired from the binder callback; `requestMtu` from
  `setMtu` on the FFI watchdog pool; `connectGatt` from another watchdog thread
- **Severity:** High · **HW gate:** 2 phones + `btsnoop`

**What:** the single most repeated rule in the Android BLE literature: *"the
Android BLE stack is single-threaded under the hood; pretend it is in your code
too — serialize every GATT operation, one at a time, per connection."* IRIS
serializes writes (HW-7) but nothing else. `set_mtu` can be called by the core
while a write is in flight; `discoverServices` runs from `onConnectionStateChange`
with no coordination with a concurrent `connectGatt` to another peer sharing the
stack. This is a strong candidate mechanism for the intermittent GATT `133` /
"characteristic not yet discovered" / silent-stall failures.

**Fix sketch:** one operation queue per `BluetoothGatt` (and a global gate for
connect, which is stack-wide): every op (connect, discoverServices, requestMtu,
write, read, disconnect) is enqueued and executed only when the previous op's
callback has fired or timed out. This is what blessed-android / Nordic's BLE
library / RxAndroidBle all do. Replace the ad-hoc `writeCompletion` /
`connectionReady` futures with the queue.

**Sources:** [Martijn van Welie — Making Android BLE work Pt 2](https://medium.com/@martijn.van.welie/making-android-ble-work-part-2-47a3cdaade07),
[Punch Through — Android BLE Connect Flow](https://punchthrough.com/how-to-android-ble-connect/),
[crickshaw.dev — Surviving GATT_ERROR 133](https://crickshaw.dev/notes/android-ble-gatt-error-133/).

---

### HV-68 — Stale GATT service cache after a peer re-advertises / restarts — no `refreshDeviceCache`

- **Fix status:** ⬜ · **Home tier:** 1
- **Area:** `AndroidBleTransportAdapter` `gattCallback.onServicesDiscovered`
- **Severity:** Medium · **HW gate:** 2 phones, restart the peer app mid-session

**What:** Android caches a peer's GATT database. If phone B's app restarts (new
GATT server instance, HV-16/HV-31 territory) or its service set changes, phone A's
cached DB is stale and `onServicesDiscovered` may report the old layout —
`getService(IRIS_SERVICE_UUID)` returns null or a dead characteristic handle, and
every write fails "characteristic not yet discovered" even though B is advertising
fine. The private `gatt.refreshDeviceCache()` (via reflection) clears it; the BLE
libraries all ship this workaround.

**Fix sketch:** on a service-discovery result that lacks `IRIS_SERVICE_UUID` when
the beacon says the peer supports it, call `refreshDeviceCache()` (reflection),
disconnect, and reconnect once before giving up. Bound to one retry.

**Sources:** [Punch Through — BLE scan returns no results](https://punchthrough.com/ble-scan-returns-no-results-on-android/),
[Martijn van Welie — Making Android BLE work Pt 2](https://medium.com/@martijn.van.welie/making-android-ble-work-part-2-47a3cdaade07).

---

### HV-69 — Scan cadence: re-arm-every-pass fights the platform; adopt the "one window, ~25 s, brief overlap" pattern

- **Fix status:** ⬜ · **Home tier:** 1 · groups with HV-11 / HV-14
- **Area:** `ble.rs` `discover_peers` (calls `start_scan` every pass),
  `scan_allowed`, `ScanRestartPolicy`, `BleScanSession`
- **Severity:** High · **HW gate:** 2 phones, 10-min discovery session

**What:** the AOSP throttle is **5 `startScan` calls per 30 s → the 6th silently
no-ops for the rest of the window, no error callback** (`ScanManager`, API 24+).
The universal guidance: *"start once, run for your window, stop; if you must
restart, use a ~25 s cycle with a brief overlap so you don't miss ads."* IRIS
restarts the scan on every `discover_peers` pass (and again on every reconnect —
HV-14), which is exactly the anti-pattern. There is also an **opportunistic scan
mode** (`SCAN_MODE_OPPORTUNISTIC`) that piggybacks on other apps' scans and does
not count against the throttle — usable as a always-on low-cost baseline.

**Fix sketch:** one long-lived scan session with a 25-s-on / brief-overlap
re-arm, decoupled from the `discover_peers` call cadence (which just drains
results); a separate targeted `ScanFilter`-by-address scan for reconnecting a
known dropped peer (HV-14); consider an opportunistic-mode baseline scan that is
always on.

**Sources:** [Punch Through — Android BLE scan errors](https://punchthrough.com/android-ble-scan-errors/),
[devsflow — Android BLE background scanning limits](https://www.devsflow.ca/blog/ble-android-lessons.html),
[Nordic Android-Scanner-Compat-Library #18](https://github.com/NordicSemiconductor/Android-Scanner-Compat-Library/issues/18).

---

### HV-70 — Android 15/16 blocks background-initiated foreground-service starts — `IrisBleService.start` from the ViewModel / WorkManager may fail

- **Fix status:** ⬜ · **Home tier:** 3 · groups with HV-32
- **Area:** `IrisBleService` (`connectedDevice` FGS type — declared correctly),
  `MeshViewModel.ensureStarted` (`IrisBleService.start(appContext)`),
  `IrisBackgroundSyncWorker`, `IrisApplication`
- **Severity:** Medium–High · **HW gate:** Android 15+ phone

**What:** the manifest correctly declares `foregroundServiceType="connectedDevice"`
+ `FOREGROUND_SERVICE_CONNECTED_DEVICE`. But Android 15 (API 35) tightened
*background-initiated* FGS starts: an app in the background generally can no longer
start an FGS except via `BOOT_COMPLETED`, WorkManager expedited work, or specific
exemptions — a `startForegroundService` from a background context throws
`ForegroundServiceStartNotAllowedException`. `MeshViewModel.ensureStarted` catches
`RuntimeException` and resets `started` — so on Android 15 the mesh may simply
never come up if the app was launched into the background, or after a process
death + WorkManager wake (HV-32).

**Fix sketch:** research the current Android 15/16 exemption list; use a
`BOOT_COMPLETED` receiver + WorkManager expedited job to (re)start the FGS;
confirm the `connectedDevice` type's own exemption applies here; test the
cold-from-background path on an Android 15 device.

**Sources:** [Android — Foreground service types are required](https://developer.android.com/about/versions/14/changes/fgs-types-required),
[Android — FGS types reference](https://developer.android.com/develop/background-work/services/fgs/service-types),
[bleadvertiserapp — Android 15 Broke Your BLE App](https://bleadvertiserapp.medium.com/android-15-broke-your-ble-app-new-permission-rules-3d8cb3c9ba86).

---

### HV-71 — Verify discovery works with device Location **off** (the manifest claims `neverForLocation`)

- **Fix status:** ✅ Verified, no fix needed (BLE) · 🔓 partially closed ·
  2026-09-05 · Session 23 · P1 (vivo V2205, Android 15) + P2 (vivo 2004,
  Android 12/SDK 31). Turned Location Services fully off on both phones
  (`cmd location is-location-enabled` confirmed `false` on both, not just
  the legacy `settings put secure location_mode 0` shim), cleared logcat,
  relaunched IRIS on both, ran `test_pingpong_300char_10x` (BLE). Result:
  **fwd=10/10, rev=9/10** — one dropped round out of 20 total sends, which
  is within this transport's already-documented ordinary flakiness band
  (see Tier 1/3 entries), not a Location-off-specific regression. BLE
  discovery, connection, and bidirectional delivery all worked essentially
  normally with Location off on both API levels tested — `neverForLocation`
  is doing its job here, this bench does not exhibit the OEM
  gate-scan-results-on-Location-Services behaviour the finding worried
  about. Restored Location Services to `HIGH_ACCURACY` on both phones after.
  **Not verified**: Wi-Fi Direct discovery with Location off (moot right
  now — Wi-Fi Direct discovery is already broken with Location *on*, per
  HV-21; re-test once HV-21's fix lands). Leaving `🔓` rather than `🟢`
  since the Wi-Fi Direct half of this finding's own `**HW gate**` is
  untested, not because the BLE half needs more work.
- **Area:** manifest (`BLUETOOTH_SCAN … neverForLocation`, but also
  `ACCESS_FINE_LOCATION` is declared), `AndroidBleTransportAdapter`,
  `MeshPermissions`
- **Severity:** Medium · **HW gate:** 2 phones, Location toggled off

**What:** the manifest declares `BLUETOOTH_SCAN` with `neverForLocation` (API 31+)
*and* `ACCESS_FINE_LOCATION`. On API 31+ with `neverForLocation`, scanning should
not require the Location toggle — but some OEM builds still gate scan *results*
(not the API call) on Location Services being on, and declaring
`ACCESS_FINE_LOCATION` alongside `neverForLocation` can confuse the grant model.
Wi-Fi P2P/NSD on API ≤32 genuinely needs `ACCESS_FINE_LOCATION`. This mix has
never been tested with Location off.

**Fix sketch:** bench discovery with Location Services off on both phones across
both bench OS versions; if it fails, decide whether `ACCESS_FINE_LOCATION` can be
`maxSdkVersion="32"`-scoped (needed only for old Wi-Fi P2P) and whether to prompt
the user to enable Location.

**Sources:** [Android — BLE background / neverForLocation](https://developer.android.com/develop/connectivity/bluetooth/ble/background),
[Punch Through — BLE scan returns no results](https://punchthrough.com/ble-scan-returns-no-results-on-android/).

---

### HV-72 — Wi-Fi Aware attach/session lifecycle: another app or a Wi-Fi toggle kills the NAN session

- **Fix status:** ⬜ · **Home tier:** 7 · **HW gate:** 🔒 2 phones with NAN
- **Area:** `AndroidWifiAwareTransportAdapter`, `wifiaware.rs`
- **Severity:** Medium

**What:** `WifiAwareManager` grants a NAN session to one app at a time on most
hardware; another app attaching, or the user toggling Wi-Fi, tears IRIS's session
down (`onAwareSessionTerminated` / `AttachCallback.onAttachFailed`). NDP data-path
setup also has its own failure and channel (2.4 vs 5 GHz) considerations. Whether
IRIS recovers is untested (compounds HV-51).

**Fix sketch:** handle session-terminated by marking the transport `Unavailable`
and re-attaching with backoff; surface it in `/diag`. Lower priority — Wi-Fi
Aware is behind BLE + Wi-Fi Direct.

**Sources:** [Android — Wi-Fi Aware](https://developer.android.com/develop/connectivity/wifi/wifi-aware).

---

### HV-73 — Clock skew between phones breaks TTL, beacon freshness, and replay high-water

- **Fix status:** ⬜ · **Home tier:** 4 (cross-cutting)
- **Area:** `engine.rs` `build_text_envelope` (`ttl_seconds: 3600`,
  `timestamp: unix_now()`), `ble.rs` `discover_peers` (beacon
  `freshness_minutes` > 60 → skip), `message_engine` replay high-water,
  `expiry.rs`
- **Severity:** Medium · **HW gate:** 2 phones with deliberately skewed clocks

**What:** every timing decision uses the local wall clock. Two phones whose clocks
differ by minutes (common — no NTP guarantee, timezone/manual-set drift): the
beacon freshness check (`age_min > 60 → skip`) can reject a fresh peer or accept a
stale one; a message's `ttl_seconds` is evaluated against the *receiver's* clock,
so a receiver running fast can expire a message the sender just sent; the replay
high-water (advanced on timestamps) can be poisoned by a fast-clocked peer.

**Fix sketch:** research how Briar / Meshtastic / Bundle Protocol handle DTN time
without synchronised clocks (relative ages, sender-stamped + hop-stamped, bounded
skew tolerance). At minimum widen the freshness/TTL tolerances and clamp; consider
carrying a coarse time estimate in the beacon and computing skew per peer.

---

### HV-74 — Wi-Fi Direct TCP socket has no keepalive; a half-open socket hangs the read loop forever

- **Fix status:** ✅ Fixed (part 1 of 2 — SO_KEEPALIVE only) · HW-PENDING ·
  commit `<pending>` · 2026-09-05 · Session 23. `FramedSocketLink`'s `init`
  now sets `socket.keepAlive = true` — this covers both consumers of the
  shared `SocketDataPath` (Wi-Fi Direct's TCP-over-GO and Wi-Fi Aware's NDP
  socket), since the class is shared between them (see the file's own doc
  comment). This is standard TCP hygiene the code previously omitted
  entirely; the OS will now eventually probe a silent connection and report
  the death, surfacing through the same `IOException` path `readLoop`/`send`
  already handle. **Not done this pass**: an application-level heartbeat
  frame + a finite, frame-resetting post-handshake `soTimeout` (the fix
  sketch's other half) — that's a real protocol addition (both ends must
  agree on a heartbeat cadence and tolerance), bigger in scope, and left for
  a future session; `SO_KEEPALIVE` alone is a real, low-risk improvement on
  its own. **Not independently HW-tested**: neither Wi-Fi Direct nor Wi-Fi
  Aware has a live socket on this bench right now (Direct is blocked by the
  HV-21 discovery defect; Aware reports "not supported on this device" for
  both bench phones) — verified via the existing
  `FramedSocketLinkTest` suite (unchanged, all passing) and a clean
  `assembleDebug`, not a real half-open-socket hardware scenario.
- **Area:** `SocketDataPath.FramedSocketLink` (`readLoop` blocks in
  `input.readFully`; `socket.soTimeout = 0` after handshake; no `SO_KEEPALIVE`),
  `AndroidWifiDirectTransportAdapter.connectToGroupOwner` / `ensureGroupServer`
- **Area:** `SocketDataPath.FramedSocketLink` (`readLoop` blocks in
  `input.readFully`; `socket.soTimeout = 0` after handshake; no `SO_KEEPALIVE`),
  `AndroidWifiDirectTransportAdapter.connectToGroupOwner` / `ensureGroupServer`
- **Severity:** High · **HW gate:** 2 phones, move one out of range abruptly

**What:** after the handshake the socket read timeout is set back to `0`
(infinite). If the peer vanishes without a TCP FIN/RST (walks out of Wi-Fi range,
radio glitch — the common case), `readFully` blocks forever, the link stays in
`links`, `groupRegistry` still says the peer is a member, `p2p_send` keeps
"succeeding" into a dead socket's write buffer until it fills, and the drop is
only noticed reactively on a later write. No `SO_KEEPALIVE`, no application
heartbeat, no idle read timeout.

**Fix sketch:** `socket.keepAlive = true`; a periodic tiny heartbeat frame both
ends expect (missing 2–3 → tear down); a generous but finite post-handshake
`soTimeout` that resets on every frame. Feed into the per-link health machine
(HV-27).

**Sources:** [Android — Wi-Fi Direct](https://developer.android.com/develop/connectivity/wifi/wifi-direct)
(no keepalive guidance — this is standard TCP hygiene the code omits).

---

### HV-75 — The core relay path does **not** flood and does **not** consult a routing/neighbour table — multi-hop only works if the relay node already has a direct link to the final recipient

- **Fix status:** ⬜ · **Home tier:** 4 · **elevated — this is why "hopping never works"**
- **Area:** `message_engine/mod.rs` `deliver_or_relay` / `enqueue_relay` /
  `deliver_outbound` (calls `manager.select_transports(target_peer = final
  recipient)` — never `flood::recipients_for_flood`, never `NeighborTable`);
  comments at `mod.rs:21` ("true flooding lands in WP-4") and `:1011` / `:1945`
  ("the per-priority flood policy lives in the routing module, **which the live
  path does not call**"); `engine.rs` (Android builds a `DiscoveryManager` but
  **no** `NeighborTable` and no routing engine)
- **Severity:** Critical · **HW gate:** 🔒 3 phones

**What:** the message engine's live relay is "re-enqueue the received envelope and
`select_transports` toward its final recipient." `select_transports` filters on
`supports_unicast` — **not** on "do I have a live link to that peer." So when node
B receives A's message addressed to C and B has a BLE link to A but not to C:
`select_transports` returns the BLE transport (it supports unicast), `send()` looks
up `connections` for C's key, finds nothing, errors, requeues, and the message
TTL-expires an hour later. There is no flood to reach C via other neighbours,
because `flood.rs` / `recipients_for_flood` / `NeighborTable` are **not wired into
the live path** — they exist only for the simulator and a future work package.
`routing_hints` is decoded and never read.

**Why this is the headline for Tier 4:** the operator says "no multi-hopping, no
flood, no prophet ever tested." The deeper truth is: **on the current code,
multi-hop cannot succeed except in the degenerate case where the relay already has
a direct link to the destination** (which isn't multi-hop). Flood and PRoPHET
aren't just untested — they are not connected to the code path that runs on the
phone.

**Fix sketch (design-level, large):**
1. Wire a real `NeighborTable` into the Android engine, fed from
   `DiscoveryManager` + transport link events.
2. Route relayed messages through an actual next-hop decision:
   `flood::recipients_for_flood` for broadcast/epidemic, PRoPHET/known-path for
   addressed, over neighbours the node *actually has links to*.
3. `select_transports` must consider peer reachability, not just capability
   flags (see HV-48).
4. Only then are HV-35/36/37 (multi-hop / flood / PRoPHET on hardware) even
   meaningful to test.

This is a substantial routing-integration effort and likely its own project node
— but it is the thing standing between IRIS and its core value proposition.

---

### HV-76 — Reassembly TTL vs real fragmented-message wall-time — a slow multi-fragment message is silently dropped

- **Fix status:** ⬜ · **Home tier:** 1 · groups with HV-7
- **Area:** `ble.rs` `spawn_inbound_poller` (`recon.evict_stale(REASSEMBLY_TTL)`),
  `ble_att.rs` (`REASSEMBLY_TTL`, `Reassembler`)
- **Severity:** Medium · **HW gate:** 2 phones at range (high retransmit)

**What:** a message fragmented into N pieces at MTU 23 (HV-7), sent serially with
blocking waits (HW-7) and BLE-layer retransmits under interference, can take
several seconds to fully arrive. If total arrival time exceeds `REASSEMBLY_TTL`,
the partial reassembly slot is evicted and every fragment received so far is
discarded — the message is lost with no signal to either side (the sender saw all
writes "succeed"). `REASSEMBLY_TTL` was picked for the simulator's zero-latency
world.

**Fix sketch:** measure real fragmented-message arrival time on the bench at 1 m /
5 m / one wall; set `REASSEMBLY_TTL` to a generous multiple of the worst observed;
better, reset the TTL on *every* fragment received (idle timeout, not absolute)
and emit a diagnostic when a partial is evicted.

---

### HV-77 — Duplicate / out-of-order fragment tolerance on real radios is unverified

- **Fix status:** ⬜ · **Home tier:** 1
- **Area:** `ble_att.rs` `Reassembler::push`, `AttSegmenter`
- **Severity:** Medium · **HW gate:** 2 phones + `btsnoop` (to confirm dup/reorder)

**What:** BLE link-layer retransmits and (with HV-65 write-without-response) app-
layer resends can deliver the same fragment twice; multipath P0 (HV-47) and any
future concurrent-transport delivery can interleave fragments. Does
`Reassembler::push` idempotently handle a repeated fragment index, and does it
handle fragment 3 arriving before fragment 2? Simulator delivery is in-order and
exactly-once, so this has never been exercised.

**Fix sketch:** unit-test `Reassembler` with duplicate and reordered inputs
(L2 test); confirm on the bench with `btsnoop` showing the actual fragment
sequence. Fix `push` to be index-addressed and dup-tolerant if it isn't.

---

### HV-78 — `dumpsys`-level state is the real debugging surface and it isn't captured

- **Fix status:** ⬜ · **Home tier:** 0 · folds into HV-3 / HV-2
- **Area:** test harness / diagnostics — `dumpsys bluetooth_manager`,
  `dumpsys wifip2p`, `btsnoop_hci.log`
- **Severity:** Medium (instrumentation) · **HW gate:** n/a (tooling)

**What:** the HW-11 fix already leaned on `dumpsys wifip2p` to diagnose the S24
P2P bring-up delay — but that was ad-hoc. `dumpsys bluetooth_manager` (registered
advertisers/scanners, GATT client/server connections, bond state) and
`dumpsys wifip2p` (P2P state machine, group, peers) and the `btsnoop` HCI log
(the actual ATT/MTU/disconnect-reason timeline) are the three surfaces that
actually explain a failure. None are part of a repeatable workflow.

**Fix sketch:** the `iris_bench` harness (HV-2) captures all three automatically
before/after every test into the evidence folder (see loop §4.3). Enable
*Bluetooth HCI snoop log* on both bench phones as a one-time setup step.

**Sources:** [Charlie Anderson — how to get HCI logs from a modern Android phone](https://medium.com/@charlie.d.anderson/how-to-get-the-bluetooth-host-controller-interface-logs-from-a-modern-android-phone-d23bde00b9fa),
[NowSecure — Bluetooth packet capture with Wireshark on Android](https://www.nowsecure.com/blog/2017/02/07/bluetooth-packet-capture-on-android-4-4/).

---

### HV-79 — Controlled-mobility test method for DTN/PRoPHET (the "courier loop")

- **Fix status:** ⬜ · **Home tier:** 4 · **HW gate:** 🔒 3 phones + a person to walk
- **Area:** test methodology — feeds HV-37
- **Severity:** Medium (methodology) 

**What:** PRoPHET / store-carry-forward can only be validated with node mobility,
and "someone walks around with a phone" is not repeatable. The DTN research
literature (MeshTest testbed, controlled-mobility studies) solves this with RF
attenuator matrices — overkill for a two-phone bench, but the *principle* (a
scripted, timed mobility pattern with defined pass criteria) transfers.

**Fix sketch:** define a written **courier-loop procedure**: phones A and B fixed
and out of range of each other (one wall + distance, verified by A and B not
discovering each other); phone C carried on a fixed ~2-minute loop that brings it
into range of A, then out, then into range of B, then out; messages queued A→B
before C starts; pass = C picks up at A, carries, delivers at B, and the
delivery-predictability metric increases across loops. Timed, repeatable, logged
as an L4 manual procedure.

**Sources:** [MeshTest wireless testbed (ACM CHANTS)](https://dl.acm.org/doi/10.1145/1409985.1409996),
[On Controlled Node Mobility in DTNs](https://www.researchgate.net/publication/251466817_On_Controlled_Node_Mobility_in_Delay-Tolerant_Networks_of_Unmanned_Aerial_Vehicles).

---

### HV-80 — Advertising interval / tx power / scan mode are all fixed at the lowest setting — discovery is slow and short-range by construction

- **Fix status:** ⬜ · **Home tier:** 1
- **Area:** `AndroidBleTransportAdapter.startAdvertising`
  (`ADVERTISE_MODE_LOW_POWER`, `ADVERTISE_TX_POWER_MEDIUM`), `scanSettings`
  (`SCAN_MODE_LOW_POWER`)
- **Severity:** Medium · **HW gate:** 2 phones at range, measure time-to-discover

**What:** everything is pinned to the battery-friendly minimum:
`ADVERTISE_MODE_LOW_POWER` (~1 s advertising interval), `TX_POWER_MEDIUM`, and
`SCAN_MODE_LOW_POWER` (a short scan window on a long interval). Combined, two
phones can take tens of seconds to discover each other, and range is reduced.
That's a reasonable *background* setting but a poor *foreground, user-is-waiting*
setting, and it compounds the -85 dBm RSSI floor (HV-12).

**Fix sketch:** an adaptive profile — `ADVERTISE_MODE_BALANCED` / `LOW_LATENCY`
and `SCAN_MODE_BALANCED` / `LOW_LATENCY` when the app is foregrounded or the mesh
has zero peers, dropping to low-power once a stable link exists and the screen is
off. Bench: median time-to-first-discovery in each profile at 1 m / 5 m / one
wall.

**Sources:** [Punch Through — Android BLE guide (scan/advertise modes)](https://punchthrough.com/android-ble-guide/),
[bleadvertiserapp — BLE scan strategy / battery](https://bleadvertiserapp.medium.com/why-your-ble-app-is-draining-battery-and-the-scan-strategy-that-fixes-it-2a10d904febf).

---

## Appendix A — finding → operator-complaint map

| Operator said | Findings |
|---|---|
| "sometimes succeeds, sometimes doesn't" (BLE send) | HV-7, HV-8, HV-14, HV-27, HV-33, HV-48, HV-65, HV-66, HV-67, HV-76, HV-77 |
| "message sends through BLE sometimes, sometimes not" | HV-7, HV-9, HV-11, HV-12, HV-15, HV-68, HV-69, HV-80 |
| "with internet there is no sending/receiving" | HV-42, HV-43, HV-44, HV-45 |
| "have to write the 64-hex every time" | HV-56, HV-57, HV-62 |
| "UI sucks / textbox hides / no send button / can't see messages while typing" | HV-54, HV-55, HV-63 |
| "only one-way, unicast only, no friends" | HV-41, HV-56, HV-57, HV-58 |
| "connection establishes then lost; loses connection" | HV-14, HV-15, HV-22, HV-27, HV-31, HV-33, HV-67, HV-74 |
| "Wi-Fi Direct connections create conflict" | HV-19, HV-20, HV-25, HV-46 |
| "hopping never tested" | HV-35, HV-36, HV-37, HV-39, HV-40, **HV-75** (root cause — the live path doesn't flood or route) |
| "one sends, other doesn't receive / receives then link lost" | HV-7, HV-8, HV-13, HV-22, HV-27, HV-74, HV-76 |
| "advertising stops, beacon stops" | HV-10, HV-11, HV-31, HV-70 |
| "connection only with one peer; must keep re-discovering" | HV-13, HV-14, HV-15, HV-40, HV-50, HV-69, HV-80 |
| "docs say tested but it isn't reliable" | HV-2, HV-4, HV-6, HV-78, and the whole tracker |
| "smart-watch / Bluetooth headset — then what" | HV-18, HV-46 |
| "no persistence beyond one connection" | HV-15, HV-27, HV-32 |
| "slow / takes forever to connect or send" | HV-7, HV-23, HV-65, HV-66, HV-67, HV-80 |

## Appendix B — files in scope

```
crates/iris-core/src/transport/{ble,ble_advert,ble_att,wifi_direct,wifi_direct_serv,wifiaware,wifiaware_beacon,internet,manager,mod}.rs
crates/iris-core/src/routing/{flood,prophet,scf,scf_contact,scf_eviction,opportunistic,direct,known_path,dedup_cache,mod}.rs
crates/iris-core/src/message_engine/{mod,queue,ack,expiry,fragment,lifecycle}.rs
crates/iris-core/src/discovery/{mod,handshake,neighbor_table}.rs
crates/iris-android/src/{engine,bridge,lib,logging}.rs
crates/iris-android/src/ffi/{ble_adapter,wifi_aware_adapter,wifi_direct_adapter,mod}.rs
android/app/src/main/kotlin/iriscore/adapter/*.kt
android/app/src/main/kotlin/iriscore/service/*.kt
android/app/src/main/kotlin/iriscore/ui/**/*.kt
android/app/src/main/kotlin/iriscore/data/*.kt
android/app/src/main/kotlin/iriscore/command/*.kt
android/app/src/main/kotlin/iriscore/worker/*.kt
android/app/src/main/kotlin/iriscore/util/PeerIdCodec.kt
```

New code likely needed: an Android `InternetTransport` FFI adapter + Rust
registration, an NSD/mDNS discovery path, a `ContactStore`, an FFI `snapshot()` /
`broadcast_text()` / topology surface, a send-button + reply UI, an injectable
fault model for the sim adapters.
