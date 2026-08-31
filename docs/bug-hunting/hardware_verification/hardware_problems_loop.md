# Hardware Verification — Autonomous Loop Specification

**Owner:** Taksh Shah
**Companion:** [`hardware_problems.md`](hardware_problems.md) (state — 65 findings,
HV-1..HV-64) · [`hardware_fix_log.md`](hardware_fix_log.md) (journal — one entry
per bench session)
**Purpose:** a self-contained protocol an agent (or a human) executes repeatedly,
without this conversation's context, to take each hardware finding from
*"suspected"* to *"verified working on two physical phones, ten times in a row,
committed, documented."*

This loop is deliberately **slower and more evidence-heavy** than the other
bug-hunt loops in this directory. Those loops optimise for "green tree after every
commit." This one optimises for **"the operator can pick up two phones cold and it
works."** A `cargo build` passing means nothing here. The bar is `adb logcat`
evidence from real radios.

---

## §0 The prime directive

> **No finding is closed (`🟢`) until its behaviour has been observed working on
> at least two physical Android phones, with the `adb logcat` evidence pasted into
> the fix log, repeated enough times to show it is reliable and not lucky.**

A code change that "should fix it" and passes the simulator is marked `✅` (fixed,
hardware-pending) — never `🟢`. The gap between `✅` and `🟢` is the entire point
of this document. Historically, `✅`-equivalent claims were written into `docs/`
as if they were `🟢`, and the operator's field experience is the result.

If hardware is not on hand this session: do the research phase, write the code,
mark `✅ · HW PENDING`, and stop. Do not claim verification you did not perform.

---

## §1 Every finding passes through five phases, in order

For each finding, in this exact sequence. Do not skip phases. Do not batch phases
across findings (finish one finding's cycle before starting the next, except where
§5 explicitly allows grouping).

### Phase R — Research (why does this exist, and how is it actually solved?)

**Goal:** understand the problem deeply enough that the fix is derived, not
guessed. Guessed fixes are how this codebase accumulated HW-1 through HW-21 and
FFI-1 through FFI-19 — each a real fix, several of them stacked on the same root
cause because the earlier ones were partial.

**This phase is internet-research-first and it is not optional.** Android BLE,
Wi-Fi Direct and Wi-Fi Aware behaviour is under-specified in the official docs and
varies by OEM, chipset and OS version; the real answers live in AOSP source, in
long-form engineering write-ups (Punch Through, Martijn van Welie's "Making
Android BLE work" series, Nordic's compat library issues, `bleadvertiserapp` /
`crickshaw.dev` GATT-133 notes), in the issue trackers of the mature BLE
libraries (RxAndroidBle, blessed-android, Nordic Android BLE Library,
Android-Scanner-Compat-Library), and in how comparable mesh apps (Briar,
bitchat, Berty, Meshtastic) solved the same problem. **Use the web search and
web fetch tools aggressively:**

- Run **at least 3–5 distinct searches per finding**, from different angles
  (the symptom, the API contract, the error code, "how does <library> handle
  X", "AOSP <ClassName> <method>"). Follow the promising links with a fetch and
  read the actual content — do not stop at the search snippet.
- **Read AOSP source directly** where behaviour is undocumented: `frameworks/base`
  `ScanManager`, `GattService`, `WifiP2pServiceImpl`,
  `WifiAwareStateManager`, and the relevant CTS/VTS tests
  (`cts/tests/tests/bluetooth`, `cts/tests/tests/wifi`). AOSP is the ground
  truth for throttle counts, timeouts, and state machines.
- **Cross-check every platform claim against ≥2 independent sources** and note
  where they disagree. When a source and a bench observation conflict, the bench
  wins — but write down both and why.
- **Record the Android-version deltas.** Behaviour changes at API 24 (scan
  throttle), 26 (background scan), 29 (Wi-Fi P2P band), 31 (new BT permission
  model), 33 (`NEARBY_WIFI_DEVICES`), 34 (FGS types required), 35 (stricter
  `connectedDevice` FGS + background-start rules). State which the bench phones
  run and whether the finding is version-gated.
- **Every factual claim in the Research note cites a source** — a URL, an AOSP
  file+class, a Bluetooth Core Spec §, or a `logcat` line you captured. "It is
  known that…" is rejected in review.

Reason with the sources, do not just collect them: state the mechanism as a
causal chain, name the single assumption in the current code that does not hold,
and derive the fix from that — then sanity-check the derived fix against how the
mature libraries actually do it.

Produce, in the fix log under the finding's heading, a **Research note** covering:

1. **The mechanism.** Exactly what happens on the hardware, step by step, that
   produces the symptom. Cite the specific Android API contract
   (developer.android.com), the Bluetooth Core Spec section, the Wi-Fi Direct /
   Wi-Fi Aware spec, or the observed `logcat` line. "Android throttles scans" is
   not a mechanism; "`BluetoothLeScanner.startScan` enforces 5 starts per 30 s
   since API 24 and silently no-ops the 6th, firing
   `onScanFailed(SCAN_FAILED_SCANNING_TOO_FREQUENTLY)` async — see
   `ScanManager.java`" is.
2. **Why the current code gets it wrong.** Quote the current source (read it at
   HEAD — line numbers drift). Name the specific assumption that does not hold on
   hardware.
3. **How the problem is solved elsewhere.** Look at how a comparable app handles
   it — the code already cites bitchat-android for GATT write serialization
   (HW-7); do the same kind of comparison for this finding. Prefer 2+ independent
   sources. Note where sources disagree.
4. **What the solution must satisfy for IRIS specifically.** Constraints from
   `docs/`: the transport abstraction contract, the privacy model (MAC ≠
   identity), the battery budget, the emergency-path guarantees, the FFI seam
   rules (no `MutexGuard` across FFI, typed errors, drain-based inbound).
5. **Research outcome.** A 2–5 sentence statement of the chosen approach and the
   one or two alternatives rejected, with why. This is what a reviewer reads
   first.

If Phase R reveals the finding as written is wrong (premise disproved), mark it
`⚪ Premise disproved` with the reasoning and move on — do not fix a non-problem.

**Web research is expected in this phase.** Android Wi-Fi P2P / BLE behaviour is
under-documented and OEM-variable; the answer is often in a Stack Overflow thread,
an AOSP source file, or a blog post, not the official docs.

### Phase D — Design & implement

1. Re-read every source file the finding touches, at HEAD.
2. Write the smallest change that implements the Phase-R outcome. Do not refactor
   unrelated code. Do not fix a nearby bug you notice — log it as a new HV-##
   candidate in the fix log and leave it.
3. If the fix spans Rust core + Rust FFI + Kotlin adapter (common), change all
   layers in one coherent commit — a half-applied cross-layer fix is worse than
   none.
4. Add or extend a **fault-injection sim test** (HV-4) that reproduces the failure
   mode and asserts recovery. This is the CI regression guard. It does not replace
   the phone test.
5. `cargo build --workspace` and `cargo test -p iris-core -p iris-android`
   (narrow scope) must pass. `./gradlew :app:assembleDebug` must pass if any
   Kotlin changed.

### Phase T — Test on hardware

This is the phase the other loops in this directory do not have.

**Setup (once per session):**
- Two physical Android phones, call them P1 and P2. Record model, Android version,
  and (where knowable) Bluetooth/Wi-Fi chipset in the log.
- Install the build under test on both (`./gradlew :app:installDebug` or
  `adb -s <serial> install -r`).
- `adb -s <P1serial> logcat -c && adb -s <P2serial> logcat -c` (clear).
- Start capturing both: `adb -s <serial> logcat -v time > p1.log` (and p2.log).
- Grant all permissions on both. Confirm `/node` shows a stable id on each.

**Per-finding hardware test:**
- Each finding in `hardware_problems.md` has (or you must write) an **HW
  verification** procedure — the exact steps and the pass criteria. Follow it
  verbatim.
- The default reliability bar: **the scenario must succeed 10 consecutive times.**
  For findings about a specific failure mode (e.g. "scan-backoff lockout"), also
  run the procedure that *used to* trigger the failure and confirm it no longer
  does, 5×.
- Vary the physical conditions where the finding is distance/RF-sensitive: 1 m,
  ~5 m, one wall, and (for range findings) to the point of failure — record where
  it breaks.
- Alternate roles: which phone powers on first, which advertises first, which
  sends first. Symmetry bugs (HV-19 GO/GO) only show when you swap.
- Save the relevant `logcat` excerpts (both phones, time-aligned) into the log
  entry. Redact nothing technical; redact only real message content if the
  operator used real text.

**If it fails on hardware:** the fix is not done. Go back to Phase R with the new
`logcat` evidence — you have learned something the research missed. Record the
failed attempt in the log (`❌ Attempt N: <what happened>, <logcat snippet>`).
Do **not** mark the finding `✅`. Do not commit a fix that failed its hardware
test unless it is a strict improvement that unblocks testing something else (say
so explicitly).

### Phase I — Iterate

- Most hardware findings will not pass Phase T on the first attempt. Loop R→D→T
  until the reliability bar is met, or until you have made 3 serious attempts —
  after 3, stop and write a **blocker analysis** in the log (what you tried, what
  each attempt's `logcat` showed, what you now believe the real mechanism is,
  what you would try next / what help or hardware you need). Mark `🔒 Blocked`
  with a one-line reason and move to the next finding.
- When a finding's fix turns out to depend on another finding's fix (very common
  in this area — e.g. HV-14 scan-backoff interacts with HV-15 auto-reconnect),
  note the dependency in both entries and fix the prerequisite first.

### Phase C — Close

Only when the reliability bar is met on hardware:

1. **Commit** (format in §6). One finding (or one §5 group) per commit.
2. **Update `hardware_problems.md`:** flip the finding's `Fix status` to
   `🟢 HW-verified · commit <hash> · <date> · P1=<model/ver> P2=<model/ver>`.
   Update the Progress Tracker table counts and `Last updated`.
3. **Update the relevant `docs/`:** the "confirmed working" claims in
   `docs/implementation/*_VERIFICATION.md`, `docs/transports/*`,
   `docs/testing/SYSTEM_TEST_REPORT.md`, and `docs/testing/ANDROID_BUILD_STATUS.md`
   must reflect what was *actually* verified — the device models, the date, the
   scenario, the reliability observed, and the known remaining limitations. If a
   doc currently overclaims, correct it in the same commit.
4. **Fix log entry** is complete: Research note, attempts, final `logcat`
   evidence, commit hash, docs updated.

---

## §2 Tier order and gating

Run tiers in this order. The gate between tiers is real — do not jump ahead.

1. **Tier 0 (HV-1..6, HV-78) — instrumentation & the test framework first.** You
   cannot verify Tier 1+ without being able to see what the radios do *and* a
   repeatable way to drive them. HV-1 (test compiles), HV-2 (**the Mobly
   `iris_bench` harness + `IrisTestSnippet` APK + evidence pipeline — §4**), HV-3
   (diagnostic `/diag` + `snapshot()` surface), HV-4 (fault-injection sim),
   HV-78 (`dumpsys`/`btsnoop` capture) are prerequisites for everything.
   **Gate to Tier 1:** `cargo test -p iris-android` runs in CI; the L1 sim fault
   model exists; `iris_bench` can drive two phones from the laptop and a trivial
   "P1 sends, P2 receives" test passes 10/10 with the evidence pipeline emitting
   `btsnoop` + `logcat` + `dumpsys` + `meshSnapshot` into the session folder.
2. **Tier 1 (HV-7..18) — BLE single-hop.** The foundation. Two phones, one hop.
   **Gate to Tier 2:** HV-7 and HV-8 `🟢` (a 300-char message goes both ways
   10/10); HV-10, HV-11, HV-14, HV-15 `🟢` or `🔒` with analysis (advertising and
   scanning survive reconnect churn); a 30-minute two-phone session shows zero
   unexplained link losses.
3. **Tier 3 (HV-27..34) — lifecycle.** Do this **before** Tier 2. Reason:
   Wi-Fi Direct group conflict (Tier 2) is hard to diagnose if BLE links are
   still dropping unpredictably underneath. Connection health (HV-27),
   auto-reconnect (HV-15/HV-33), and the airplane-mode/Bluetooth-toggle recovery
   (HV-31) make every subsequent test reproducible.
   **Gate to Tier 2:** toggling Bluetooth off/on, airplane mode, and the RETRY
   button all recover the mesh to a working state, verified on hardware.
4. **Tier 2 (HV-19..26) — Wi-Fi Direct.** The GO/GO election (HV-19) is the big
   one and needs a research phase of its own. **Gate to Tier 6:** cold-start both
   phones 10× (alternating power-on order) → exactly one group forms → messages
   flow both ways every time.
5. **Tier 6 (HV-46..50) — concurrent-radio coexistence.** Needs Tiers 1+2 working
   so you can actually run BLE and Wi-Fi hot at once and measure the interference.
6. **Tier 8 (HV-54..64) — Shell UX.** Can be done **in parallel** with the radio
   tiers by a second person — it barely touches the transport code. HV-54, HV-55
   (composer + send button) are trivial and high-impact; do them first, any time.
   HV-56/57/58 (contacts, reply, delivery status) are larger and should follow the
   radio work enough to have real peer data and real ACKs to display.
7. **Tier 4 (HV-35..41) — multi-hop.** 🔒 **Gated on a 3rd phone.** Prepare the
   research and the bench procedure now; execute when hardware is available. HV-41
   (broadcast primitive) and HV-40 (topology surface) can be built and unit-tested
   on 2 phones ahead of the 3-phone session.
8. **Tier 5 (HV-42..45) — internet transport.** This is a **build-out**, not a
   fix. Treat HV-42 as its own mini-project with its own design doc
   (`docs/implementation/INTERNET_ANDROID_DESIGN.md`) before writing code. It does
   not block the mesh tiers and the mesh tiers do not block it — run it in
   parallel once Tier 0 is done.
9. **Tier 7 (HV-51..53) — Wi-Fi Aware.** Lowest priority. BLE + Wi-Fi Direct are
   the transports that (partly) work. Only do Tier 7 if both bench phones have NAN
   hardware and Tiers 1–6 are solid. Otherwise document and defer.

---

## §3 Research standards (Phase R detail)

Because this is the phase that has historically been shortchanged:

- **Name the source.** Every claim about platform behaviour cites something: an
  Android API doc URL, an AOSP file + class, a Bluetooth Core Spec §, a named blog
  post / SO answer, or a `logcat` line you captured. "It's known that…" is not
  acceptable.
- **Distinguish spec from OEM reality.** Android Wi-Fi P2P especially: the docs
  describe an ideal; Samsung/Xiaomi/Oppo/Pixel stacks each deviate. The code
  comments (HW-11..16) are a record of this. When the spec and a bench observation
  disagree, the bench wins — document both.
- **Check the Android version deltas.** Behaviour changes at API 24 (scan
  throttle), 26 (background scan limits), 29 (Wi-Fi P2P band), 31 (new BT
  permissions), 33 (`NEARBY_WIFI_DEVICES`), 34 (receiver export flag). Record
  which versions the bench phones run and whether the finding is version-gated.
- **Look for the "already solved" answer.** bitchat, Briar, Berty, Meshtastic,
  the Nearby Connections API internals, and AOSP CTS tests have all fought these
  battles. Cite what they do.
- **State the IRIS-specific constraint explicitly.** e.g. "Briar bonds every peer;
  IRIS's privacy model forbids treating the MAC as identity, so we cannot — we
  need X instead."
- Write the Research note **before** touching code. If you find yourself in Phase
  D without a Research note, stop and go back.

---

## §4 Test methodology & automation framework — use this, not ad-hoc `adb`

The naive approach — flash both phones, drive one by hand or with `adb shell
monkey`, tap through the UI, watch two interleaved `logcat` streams, repeat —
is slow, unrepeatable, misses timing bugs, and produces evidence nobody can
re-run. **We do not do that.** Phase T uses the host-driven, multi-device
automation stack below. Set it up once (it's a Tier-0 deliverable, tracked as
HV-2's fix); after that every finding's hardware test is a Python function that
runs in seconds and emits a structured result.

### §4.1 The framework: Mobly + Mobly Snippet Lib

**[Mobly](https://github.com/google/mobly)** is Google's open-source Python
framework for *host-driven, end-to-end, multi-device* tests — it is exactly what
AOSP itself uses to validate Bluetooth and Wi-Fi
([source.android.com/docs/core/tests/mobly](https://source.android.com/docs/core/tests/mobly)).
A Mobly test runs on a laptop, holds handles to N connected phones at once, and
calls into each phone through a small **snippet APK** built with
**[Mobly Snippet Lib](https://github.com/google/mobly-snippet-lib)** — the APK
exposes chosen device-side methods as an RPC surface the Python test calls
directly. `[google/mobly-bundled-snippets](https://github.com/google/mobly-bundled-snippets)`
already wraps a lot of the Android API (Bluetooth enable/disable, BLE scan &
advertise, Wi-Fi, Wi-Fi P2P) and
`[google/mobly-bluetooth-ref-validation](https://github.com/google/mobly-bluetooth-ref-validation)`
is a full BLE/LE-Audio multi-device suite to copy patterns from.

Why this and not alternatives:
- **UI Automator / Espresso / `adb monkey`** — single-device, UI-coupled, can't
  coordinate "phone A advertises while phone B scans and connects." Wrong tool.
- **Firebase Test Lab** — has multi-device but the device matrix and RF
  environment aren't ours; useless for "two phones 1 m apart through a wall."
- **CTS/VTS** — validates the *platform*, not our app. We cite CTS/VTS tests as
  research (they encode the real API contracts) but don't run them.
- **Raw `adb` scripts** — what we're replacing: no structured results, no
  retry/iteration accounting, no device abstraction, reinvents everything Mobly
  gives us.

### §4.2 What we build (once)

1. **`IrisTestSnippet` APK** (in `android/app/src/androidTest` or a sibling
   `:testsnippet` module) using Mobly Snippet Lib, exposing the IRIS-specific
   hooks the black-box Android API can't give us:
   - `startMesh()` / `stopMesh()` / `meshSnapshot()` → the HV-3 `/diag` struct
     as JSON (transports, per-transport state, live links + MTU, discovered
     peers, scan-backoff state, last-N send outcomes).
   - `sendText(peerHex, text, priority)` → returns the wire message id.
   - `broadcastText(text, priority)` (once HV-41 lands).
   - `awaitDelivered(messageId, timeoutMs)` → blocks on the inbox listener,
     returns `{delivered, latencyMs, hopCount, transport}`.
   - `nodeId()` → this phone's 64-hex id (so the test wires A→B addressing
     automatically — no human copying hex).
   - `forceScanRestart()`, `dropAllLinks()`, `setBluetooth(on)`,
     `setAirplane(on)` → to drive the failure-mode findings deterministically.
   - Counter/metric getters for HV-6 (sent/delivered/expired/relayed per
     transport, link lifetimes, group-formation success rate).
2. **`iris_bench/` Python package** (host side): a `MoblyTestBase` subclass with
   `setup_class` registering `min_number=2` (or 3) Android controllers, loading
   the snippet on each, granting permissions, and clearing/capturing logs; plus
   helpers `pair_ids(a, b)`, `assert_delivered_n_times(a, b, n, payload)`,
   `with_conditions(distance=…, walls=…)` (prompts the operator to physically
   place the phones, then proceeds), and a results emitter that writes one CSV
   row per run into the session's evidence folder.
3. **One test module per tier** (`test_tier1_ble.py`, `test_tier2_wifidirect.py`,
   …). Each finding gets one test function whose docstring is its HV verification
   procedure. `@repeat(10)` / Mobly's built-in per-iteration result tracking
   gives the 10/10 reliability bar for free, with a per-iteration pass/fail table
   in the HTML report.

### §4.3 The evidence pipeline (runs automatically per test)

For every hardware test run, the harness captures — without anyone remembering to:

- **`btsnoop` HCI logs from both phones.** Enable *Bluetooth HCI snoop log* in
  Developer Options on each bench phone once; the harness pulls
  `/data/misc/bluetooth/logs/btsnoop_hci.log` (path varies by OEM — the harness
  probes the known locations) at the end of each test. This is the timeline
  ordinary `logcat` misses: the actual ATT writes, MTU exchange, connection
  events, disconnect reasons. Open in Wireshark or `btmon`; Android Studio has a
  built-in viewer. For live view, Wireshark's *Android Bluetooth Btsnoop*
  external interface works over USB without root.
  ([HCI log guide](https://medium.com/@charlie.d.anderson/how-to-get-the-bluetooth-host-controller-interface-logs-from-a-modern-android-phone-d23bde00b9fa),
  [Wireshark BT analysis](https://www.nowsecure.com/blog/2017/02/07/bluetooth-packet-capture-on-android-4-4/))
- **Time-aligned `logcat`** from both phones, filtered to `iris.*` +
  `BluetoothGatt` + `WifiP2p*` + `bt_stack`, merged by wall clock.
- **`dumpsys bluetooth_manager` and `dumpsys wifip2p`** snapshots before and
  after (state machine, registered advertisers/scanners, GATT connections, P2P
  state — the HW-11 comment already relies on `dumpsys wifip2p`).
- **The `meshSnapshot()` JSON** from both phones at the moment of pass/fail.
- Everything lands in `docs/bug-hunting/hardware_verification/evidence/session-NN/HV-n/`
  and the relevant excerpt is pasted into the fix log.

### §4.4 The four-layer test pyramid (fastest feedback first)

Run a fix through these in order; only the last one is slow.

| Layer | What | Speed | Catches |
|---|---|---|---|
| **L1 — sim fault-injection** (HV-4) | `cargo test` with the injectable loss/latency/disconnect/BUSY/MTU-change model | seconds, in CI | logic regressions, "does IRIS recover from the failure mode at all" |
| **L2 — snippet unit** | Robolectric / JVM tests of `FrameCodec`, handshake framing, `peerHandleFor` idempotency, UUID parity across the FFI boundary | seconds, in CI | the pure-logic adapter bugs (the HW-1/FFI-1 class) |
| **L3 — Mobly 2-phone automated** | the `iris_bench` test for the finding, `@repeat(10)`, phones on the bench in a fixed rig | ~1–5 min per finding | the real thing — timing, RF, OEM behaviour, reliability |
| **L4 — manual scenario** | operator walks a phone between rooms (multi-hop), toggles airplane mode by hand, uses a real keyboard/language (HV-55), realistic loadout with a paired watch (HV-18) | minutes–hours | what can't be scripted: mobility, human input paths, physical placement |

A finding is `🟢` when **L1+L2 pass in CI and L3 hits 10/10 on two phones** (plus
L4 where the finding is inherently manual — multi-hop, mobility, UX). L3 is the
bar; L1/L2 are the regression guard so it stays fixed.

### §4.5 The physical bench rig

- Two (three for Tier 4) phones on a board with **fixed, marked positions**:
  0.3 m, 1 m, 3 m, and a "one drywall wall between" slot (a partition or a
  known interior wall). Repeatability requires the phones not move between runs.
- A **known-noisy slot**: near a running 2.4 GHz Wi-Fi AP + an active BT audio
  stream, for HV-9 (ambient `0xFFFF`), HV-46 (coex), HV-18 (contention).
- Each phone: same OS build recorded, *Bluetooth HCI snoop log* on, *Stay awake
  while charging* on, battery-optimisation exemption granted to IRIS, screen
  brightness fixed, on a powered USB hub (so `adb` and charge are stable through
  a long session).
- Record each phone's model, exact Android build number, and — from
  `adb shell dumpsys bluetooth_manager | grep -i "controller\|chipset"` and
  `/vendor` — the BT/Wi-Fi chipset where obtainable. Coex behaviour (HV-46) is
  chipset-specific; the evidence must say which chipset it was seen on.

### §4.6 What stays manual (and why that's fine)

Mobly cannot walk a phone down a hallway, cannot press a specific third-party
keyboard's Enter key, cannot pair a smartwatch. Tier 4 (multi-hop / mobility),
HV-55 (keyboard/IME paths), HV-18 (headset+watch loadout), and the UX "does this
*feel* right" checks are L4 manual. Define each as a **written scripted
procedure** with explicit pass criteria (the "courier loop" for HV-37, the
"IME matrix" for HV-55) so a manual run is still repeatable and its result is
still a row in the evidence file — just executed by a person, not Python.

### §4.7 Cadence

- Every push: L1 + L2 in CI (required gate — this is HV-2's fix).
- Every bench session: the `iris_bench` L3 suite for the active tier + any
  finding under iteration, on the rig, with the full evidence pipeline.
- Before any `docs/` "verified" claim: L3 green **and** the L4 manual procedure
  for that scenario run and logged.
- Nightly (once the rig is permanent): the full `iris_bench` suite as a
  regression watch, so a fix that quietly breaks another tier's finding is
  caught the next morning, not the next manual session.

---

## §5 Findings that must be worked together

| Group | Findings | Why |
|---|---|---|
| BLE MTU & fragmentation | HV-7 + HV-8 | Central-side and peripheral-side MTU are the same mechanism; fixing one and not the other just moves the failure to the reply direction. |
| Scan lifecycle | HV-11 + HV-14 + HV-15 | Scan-failure feedback, backoff lockout, and auto-reconnect are one state machine. A fix to any one in isolation shifts the failure. |
| Advertising resilience | HV-10 + HV-31 | Advertising retry and Bluetooth-toggle recovery share the "rebuild the advertiser" path. |
| Wi-Fi Direct group | HV-19 + HV-20 + HV-22 | GO election, stable credentials, and re-formation-after-loss are one design; do the research once and land them together. |
| DNS-SD discovery | HV-21 + (HV-13 if identity moves to BLE control plane) | If the fix for HV-21 is "carry identity over BLE, not the P2P TXT record," HV-13's synthetic-id problem changes shape. |
| Internet build-out | HV-42 + HV-43 + HV-45 | A transport with no discovery and no connectivity signal is not usable; land them as one feature. |
| Composer | HV-54 + HV-55 | Same file, same PR; the send button changes the composer layout that HV-54 also touches. |
| Addressing UX | HV-56 + HV-57 + HV-62 | Contacts, reply-to-set-recipient, and onboarding all consume the same `ContactStore` + discovered-peers surface. |
| Delivery status | HV-58 + HV-49 | Surfacing "failed/expired" in the UI requires the core to stop silently looping and emit a terminal state. |
| Per-link health | HV-27 + HV-48 | Selection can only prefer a healthy link if per-link health exists. |

---

## §6 Commit & safety conventions

**Per-finding commit:**
```
fix(hw/<area>): <finding title>

Research outcome: <1–2 sentences — the mechanism and the chosen approach>
Hardware verification: <P1 model/ver> + <P2 model/ver>, <scenario>, <N/N runs>
  — or —
Hardware verification: PENDING (no devices this session)

Finding: HV-<n> (Tier <t>)
Ref: docs/bug-hunting/hardware_verification/hardware_problems.md
```

**Build-out commit (Tier 5, new transport):** normal `feat(...)` conventions; land
the design doc first.

**Doc-correction commit (part of Phase C):**
```
docs(hw): correct <doc> to match what was actually verified for HV-<n>

<doc> claimed <X>; hardware testing showed <Y>. Updated the claim, the
device list, the date, and the known-limitations section.
```

Every commit message ends with:
```
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
```

**Safety limits:**
- **Never mark `🟢` without `logcat` evidence in the log.** This is the one rule
  that matters.
- **Never claim a doc is "verified" for a scenario you did not run on hardware.**
  Correcting an overclaim is always in scope.
- **One finding (or one §5 group) per commit.** Keep diffs reviewable.
- **The tree must build after every commit** (`cargo build --workspace`,
  `./gradlew :app:assembleDebug` if Kotlin changed). A fix that fails its
  *hardware* test but builds and improves observability may be committed **with an
  explicit note that it is not yet verified** — hardware iteration needs the
  instrumentation in place.
- **No destructive git.** No `reset --hard`, `push --force`, `clean -f` without
  explicit operator instruction. This loop never pushes.
- **Max 4 findings per bench session**, then stop and write the session closeout.
  Hardware sessions are long; keep each one's evidence coherent.
- **If 3 consecutive findings fail their hardware test in one session, stop.**
  Something upstream regressed (a build issue, a phone OS update, a dead battery
  in a test phone) — investigate the common cause before continuing.
- **Do not delete or weaken a test to make a build pass.** If a sim test now
  contradicts hardware reality, the *test* is wrong — fix it to model reality
  (Phase D step 4) and say so in the log.
- **Scope:** files listed in `hardware_problems.md` Appendix B. A fix that needs
  changes outside that set (e.g. a new Room dependency for `ContactStore`, a new
  crate for the internet transport) is fine but call it out in the log and the
  commit.

---

## §7 How to run this

At the start of any session:

1. Read `hardware_problems.md` Progress Tracker → find the active tier and the
   next `⬜` / `🔬` finding in it, respecting the §2 gates and §5 groups.
2. Read `hardware_fix_log.md` — did the last session leave a finding mid-iteration
   (`❌ Attempt N`)? Resume from its Research note with the failure evidence.
3. Confirm working directory and clean `git status`.
4. Confirm what hardware is available this session. If none: research + code +
   `✅ HW PENDING` only.
5. Work the loop (§1) for up to 4 findings.
6. Write the session closeout in the log: devices used, findings advanced, `🟢`
   count delta, blockers opened, what the next session should pick up.

Recommended invocation — a `/loop` with dynamic self-pacing, re-entering this file
each wake, prompt equivalent to:

> Continue the hardware verification loop in
> `docs/bug-hunting/hardware_verification/hardware_problems_loop.md`. Read the
> Progress Tracker in `hardware_problems.md` for the active tier and next finding.
> Follow the five phases (§1) in order — Research before code (Phase R is
> internet-research-first, §1+§3), hardware test before closing via the Mobly
> `iris_bench` harness (§4). Respect the tier gates (§2) and coordinated groups
> (§5). If no physical phones are available this session, do Research +
> Design/implement + L1/L2 tests only
> and mark `✅ HW PENDING`. Never mark a finding `🟢` without `adb logcat`
> evidence in the fix log. Stop after 4 findings or on the 3-consecutive-failures
> condition and write the session closeout.

Do not run this loop unattended across many wakes. Hardware verification is
inherently a human-in-the-loop activity (someone has to hold the phones, walk
between rooms, toggle airplane mode). The agent's job between bench sessions is
the Research and Design/implement phases; the human's job is Phase T.
