"""Tier-1 BLE single-hop — MTU & fragmentation (HV-7 + HV-8).

Loop §2 gate to Tier 2: "a 300-char message goes both ways 10/10" with no
unexplained link teardown. Loop §5 group: HV-7 (one failed ATT write tears the
whole link down; no per-fragment retry) + HV-8 (peripheral-role / inbound
connections never negotiate MTU — replies fragment at 23 bytes).

Each test docstring is its HV verification procedure (loop §4.2).

Run:  python -m iris_bench --tests test_tier1_ble.Tier1Ble.<method>
"""

import json
import re

from mobly import asserts
from mobly import test_runner

from iris_bench.base import IrisBenchBase

_ITERATIONS = 10
_DELIVER_TIMEOUT_MS = 90_000

# 300 printable chars — forces multi-frame at MTU 23 (~28 frames), single frame
# at MTU 517. ASCII only so byte length == char length.
_PAYLOAD_300 = ("HV7-" + "".join(chr(0x30 + (i % 10)) for i in range(292)) + "-END")
assert len(_PAYLOAD_300) == 300

# logcat signatures that mean "the link was torn down mid-test" (HV-7 symptom).
_CHURN_PATTERNS = [
    r"close_peer",
    r'event="discovery\.connect_failed"',
    r'event="discovery\.connect_backoff',
    r"msg\.delivery_failed",
    r"characteristic .* not yet discovered",
]


class Tier1Ble(IrisBenchBase):

    def setup_test(self):
        self.start_mesh_both()

    def teardown_test(self):
        self.stop_mesh_both()

    # ---- helpers ---------------------------------------------------------

    def _logcat_since_cleared(self, ad):
        return ad.adb.logcat(["-d"]).decode("utf-8", "replace")

    def _churn_hits(self, text):
        hits = {}
        for pat in _CHURN_PATTERNS:
            found = re.findall(pat, text)
            if found:
                hits[pat] = len(found)
        return hits

    def _run_direction(self, src, dst, label):
        for ad in self.ads:
            ad.adb.logcat(["-c"])
        results = []
        for i in range(_ITERATIONS):
            r = self.send_and_await(
                src, dst, f"{_PAYLOAD_300}#{i}",
                priority=4, timeout_ms=_DELIVER_TIMEOUT_MS,
            )
            results.append(r)
            src.log.info("%s iter %d/%d: delivered=%s latency=%sms",
                         label, i + 1, _ITERATIONS,
                         r.get("delivered"), r.get("latencyMs"))

        self.capture_evidence(f"tier1_{label}")
        delivered = [r for r in results if r.get("delivered")]
        # payload integrity — a fragmentation bug can deliver a truncated/garbled body
        # body is _PAYLOAD_300 + "#<i>"; check the 300-char core survived intact
        _expected = {f"{_PAYLOAD_300}#{i}" for i in range(_ITERATIONS)}
        intact = [r for r in delivered if r.get("payloadUtf8", "") in _expected]
        churn = {ad.label: self._churn_hits(self._logcat_since_cleared(ad))
                 for ad in self.ads}
        lat = sorted(r["latencyMs"] for r in delivered if r.get("latencyMs", -1) >= 0)
        summary = {
            "direction": label,
            "delivered": len(delivered),
            "intact": len(intact),
            "of": _ITERATIONS,
            "median_latency_ms": lat[len(lat) // 2] if lat else None,
            "churn": churn,
        }
        src.log.info("TIER1 %s SUMMARY: %s", label, json.dumps(summary))
        return summary, results

    # ---- HV-7: central -> peripheral, 300 chars ------------------------

    def test_p1_to_p2_300char_10x(self):
        """HV-7 fwd: P1 sends a 300-char message to P2, 10 times. Pass:
        10/10 delivered, bodies intact, zero link-teardown signatures in
        either phone's logcat during the run."""
        summary, results = self._run_direction(self.p1, self.p2, "p1_to_p2")
        asserts.assert_equal(summary["delivered"], _ITERATIONS,
                             f"P1->P2 300-char must be 10/10; {summary}\n{results}")
        asserts.assert_equal(summary["intact"], _ITERATIONS,
                             f"P1->P2 bodies must be intact; {summary}")
        asserts.assert_false(
            any(summary["churn"].values()),
            f"no link teardown allowed during the run; {summary['churn']}")

    # ---- HV-8: peripheral -> central (the reply direction), 300 chars --

    def test_p2_to_p1_300char_10x(self):
        """HV-8 reply: P2 sends a 300-char message to P1, 10 times. This is
        the direction that runs at MTU 23 when P2's link to P1 is
        peripheral-only. Pass: 10/10 delivered, bodies intact, no teardown."""
        summary, results = self._run_direction(self.p2, self.p1, "p2_to_p1")
        asserts.assert_equal(summary["delivered"], _ITERATIONS,
                             f"P2->P1 300-char must be 10/10; {summary}\n{results}")
        asserts.assert_equal(summary["intact"], _ITERATIONS,
                             f"P2->P1 bodies must be intact; {summary}")
        asserts.assert_false(
            any(summary["churn"].values()),
            f"no link teardown allowed during the run; {summary['churn']}")

    # ---- ping-pong: receive then immediately reply (HV-8's real symptom) --

    def test_pingpong_300char_10x(self):
        """The 'reply fails seconds after a successful receive' symptom: P1
        sends 300 chars to P2, then P2 immediately replies 300 chars to P1.
        10 round trips. Pass: 20/20 delivered (10 each way), no teardown."""
        for ad in self.ads:
            ad.adb.logcat(["-c"])
        fwd_ok = rev_ok = 0
        for i in range(_ITERATIONS):
            a = self.send_and_await(self.p1, self.p2, f"{_PAYLOAD_300}>{i}",
                                    timeout_ms=_DELIVER_TIMEOUT_MS)
            if a.get("delivered"):
                fwd_ok += 1
            b = self.send_and_await(self.p2, self.p1, f"{_PAYLOAD_300}<{i}",
                                    timeout_ms=_DELIVER_TIMEOUT_MS)
            if b.get("delivered"):
                rev_ok += 1
            self.p1.log.info("round %d: fwd=%s rev=%s", i + 1,
                             a.get("delivered"), b.get("delivered"))
        self.capture_evidence("tier1_pingpong")
        churn = {ad.label: self._churn_hits(self._logcat_since_cleared(ad))
                 for ad in self.ads}
        self.p1.log.info("PINGPONG: fwd=%d/10 rev=%d/10 churn=%s",
                         fwd_ok, rev_ok, json.dumps(churn))
        asserts.assert_equal((fwd_ok, rev_ok), (_ITERATIONS, _ITERATIONS),
                             f"ping-pong must be 10/10 each way; fwd={fwd_ok} rev={rev_ok}")
        asserts.assert_false(any(churn.values()),
                             f"no teardown during ping-pong; {churn}")


    # ---- HV-11/14/15: scan lifecycle — sustained session, fast recovery -----

    def test_sustained_session_recovers_fast(self):
        """HV-14/HV-15: a 40-round 300-char ping-pong. Link drops happen
        naturally under bidirectional load (HV-96); each must recover fast
        (targeted reconnect from the cached address + fast scan cadence), not
        after the 30 s slow-scan interval. Pass: >=95% delivered each way, no
        `msg.delivery_failed`, and every gap between a drop and the next
        delivery is < 15 s."""
        import re as _re
        import time

        _ROUNDS = 40
        for ad in self.ads:
            ad.adb.logcat(["-c"])
        fwd_ok = rev_ok = 0
        for i in range(_ROUNDS):
            a = self.send_and_await(self.p1, self.p2, f"{_PAYLOAD_300}>{i}",
                                    timeout_ms=_DELIVER_TIMEOUT_MS)
            if a.get("delivered"):
                fwd_ok += 1
            b = self.send_and_await(self.p2, self.p1, f"{_PAYLOAD_300}<{i}",
                                    timeout_ms=_DELIVER_TIMEOUT_MS)
            if b.get("delivered"):
                rev_ok += 1
            if (i + 1) % 10 == 0:
                self.p1.log.info("round %d/%d: fwd=%d rev=%d", i + 1, _ROUNDS,
                                 fwd_ok, rev_ok)

        self.capture_evidence("tier1_sustained")
        churn = {ad.label: self._logcat_since_cleared(ad) for ad in self.ads}
        failed = {k: v.count("msg.delivery_failed") for k, v in churn.items()}
        # recovery gaps: from a server disconnect (newState=0) to the next
        # msg.delivered on that phone.
        gaps = []
        for lbl, txt in churn.items():
            ev = []
            for m in _re.finditer(
                r"(\d\d:\d\d:\d\d\.\d+).*?(newState=0|msg\.delivered)", txt):
                ts = m.group(1)
                secs = (int(ts[0:2]) * 3600 + int(ts[3:5]) * 60
                        + float(ts[6:]))
                ev.append((secs, m.group(2)))
            last_drop = None
            for secs, kind in ev:
                if kind == "newState=0":
                    last_drop = secs
                elif kind == "msg.delivered" and last_drop is not None:
                    gaps.append(round(secs - last_drop, 1))
                    last_drop = None
        self.p1.log.info("SUSTAINED: fwd=%d/%d rev=%d/%d failed=%s recovery_gaps=%s",
                         fwd_ok, _ROUNDS, rev_ok, _ROUNDS, failed, sorted(gaps))

        asserts.assert_true(fwd_ok >= _ROUNDS * 0.95 and rev_ok >= _ROUNDS * 0.95,
                            f"delivery must be >=95% each way; fwd={fwd_ok} rev={rev_ok}")
        asserts.assert_equal(sum(failed.values()), 0,
                             f"no msg.delivery_failed allowed; {failed}")
        slow = [g for g in gaps if g > 15]
        asserts.assert_false(slow, f"every drop must recover in <15s; slow={slow}")


    def test_30min_zero_loss_session(self):
        """§2 Tier-1→Tier-2 gate: a 30-minute two-phone session must show
        ZERO unexplained link losses. Alternating 300-char P1<->P2 every ~12 s
        for 30 min (~150 rounds). Pass:
          - 100% delivered each way (a resilience mesh at 1 m on a bench must
            not drop a single message in half an hour),
          - 0 `msg.delivery_failed`,
          - every `close_peer` / server-disconnect is followed by a delivery
            within 15 s (an *explained*, recovered loss is allowed; an
            unrecovered one is not),
          - no `discovery.connect_backoff` lockout that outlasts the session.
        This is a long test (~32 min). Run it as its own invocation."""
        import re as _re
        import time

        _MINUTES = 30
        _PERIOD_S = 12
        deadline = time.monotonic() + _MINUTES * 60

        for ad in self.ads:
            ad.adb.logcat(["-c"])
        prime = self.send_and_await(self.p1, self.p2, "prime", timeout_ms=_DELIVER_TIMEOUT_MS)
        asserts.assert_true(prime.get("delivered"), f"link must be up first; {prime}")

        rnd = 0
        fwd_ok = rev_ok = fwd_n = rev_n = 0
        while time.monotonic() < deadline:
            rnd += 1
            t0 = time.monotonic()
            a = self.send_and_await(self.p1, self.p2, f"{_PAYLOAD_300}>{rnd}",
                                    timeout_ms=_DELIVER_TIMEOUT_MS)
            fwd_n += 1
            fwd_ok += 1 if a.get("delivered") else 0
            b = self.send_and_await(self.p2, self.p1, f"{_PAYLOAD_300}<{rnd}",
                                    timeout_ms=_DELIVER_TIMEOUT_MS)
            rev_n += 1
            rev_ok += 1 if b.get("delivered") else 0
            if rnd % 10 == 0:
                self.p1.log.info("30MIN round %d (%.0f min left): fwd=%d/%d rev=%d/%d",
                                 rnd, (deadline - time.monotonic()) / 60,
                                 fwd_ok, fwd_n, rev_ok, rev_n)
            spent = time.monotonic() - t0
            if spent < _PERIOD_S:
                time.sleep(_PERIOD_S - spent)

        self.capture_evidence("tier1_30min")
        churn = {ad.label: self._logcat_since_cleared(ad) for ad in self.ads}
        failed = {k: v.count("msg.delivery_failed") for k, v in churn.items()}
        drops = {k: v.count("close_peer") for k, v in churn.items()}
        backoff = {k: v.count("discovery.connect_backoff") for k, v in churn.items()}

        # recovery gaps: server disconnect (newState=0) -> next msg.delivered.
        gaps = []
        for _lbl, txt in churn.items():
            ev = []
            for m in _re.finditer(r"(\d\d:\d\d:\d\d\.\d+).*?(newState=0|msg\.delivered)", txt):
                ts = m.group(1)
                secs = int(ts[0:2]) * 3600 + int(ts[3:5]) * 60 + float(ts[6:])
                ev.append((secs, m.group(2)))
            last_drop = None
            for secs, kind in ev:
                if kind == "newState=0":
                    last_drop = secs
                elif kind == "msg.delivered" and last_drop is not None:
                    gaps.append(round(secs - last_drop, 1))
                    last_drop = None

        dupes = {k: v.count("msg.dropped_duplicate") for k, v in churn.items()}
        self.p1.log.info(
            "30MIN SESSION: rounds=%d fwd=%d/%d rev=%d/%d msg_delivery_failed=%s "
            "close_peer=%s backoff=%s dropped_dupes=%s recovery_gaps=%s",
            rnd, fwd_ok, fwd_n, rev_ok, rev_n, failed, drops, backoff, dupes, sorted(gaps))

        # The §2 gate is "zero unexplained LINK losses" — not zero
        # `msg.delivery_failed` (that counter also covers ACKs, tracked as HV-102).
        asserts.assert_equal(fwd_ok, fwd_n, f"every fwd user message must deliver; {fwd_ok}/{fwd_n}")
        asserts.assert_equal(rev_ok, rev_n, f"every rev user message must deliver; {rev_ok}/{rev_n}")
        asserts.assert_equal(sum(drops.values()), 0,
                             f"no BLE link teardown (close_peer) in 30 min; {drops}")
        asserts.assert_equal(sum(backoff.values()), 0,
                             f"no connect-backoff lockout in 30 min; {backoff}")
        unrecovered = [g for g in gaps if g > 15]
        asserts.assert_false(unrecovered,
                             f"every link drop must recover in <15 s; slow={unrecovered}")

    def test_forced_drop_reconnect_10x(self):
        """HV-14/HV-15: prime the link, then `dropAllLinks()` on BOTH phones
        (radio stays up — advertising/scanning continue) and time how long
        until a fresh message delivers again. 10 cycles. Pass: every recovery
        < 15 s (targeted reconnect from the cached address + fast scan cadence,
        not the 30 s slow-scan interval), and no `msg.delivery_failed`.
        Dropping both sides avoids the HV-94 zombie-link case (one side keeps a
        stale handle) — that is its own finding."""
        import time
        _CYCLES = 10
        _BUDGET_S = 15

        for ad in self.ads:
            ad.adb.logcat(["-c"])
        r = self.send_and_await(self.p1, self.p2, "prime", timeout_ms=_DELIVER_TIMEOUT_MS)
        asserts.assert_true(r.get("delivered"), f"link must be up first; {r}")

        recoveries = []
        for i in range(_CYCLES):
            self.p1.iris.dropAllLinks()
            self.p2.iris.dropAllLinks()
            t0 = time.monotonic()
            ok = False
            while time.monotonic() - t0 < _BUDGET_S + 5:
                rr = self.send_and_await(self.p1, self.p2, f"reconnect-{i}",
                                         timeout_ms=6000)
                if rr.get("delivered"):
                    ok = True
                    break
                time.sleep(1)
            dt = round(time.monotonic() - t0, 1)
            recoveries.append(dt)
            self.p1.log.info("drop %d/%d: recovered=%s in %ss", i + 1, _CYCLES, ok, dt)
            asserts.assert_true(ok, f"drop {i}: no recovery within {_BUDGET_S + 5}s")

        self.capture_evidence("tier1_forced_drop")
        churn = {ad.label: self._logcat_since_cleared(ad) for ad in self.ads}
        failed = {k: v.count("msg.delivery_failed") for k, v in churn.items()}
        self.p1.log.info("FORCED-DROP: recoveries=%s failed=%s", recoveries, failed)
        asserts.assert_equal(sum(failed.values()), 0,
                             f"no msg.delivery_failed allowed; {failed}")
        slow = [d for d in recoveries if d > _BUDGET_S]
        asserts.assert_false(slow, f"every drop must recover in <{_BUDGET_S}s; slow={slow}")

    # ---- HV-10 + HV-31: adapter-state / advertising resilience ----------

    def test_bluetooth_toggle_recovers(self):
        """HV-31 (and HV-10's retry path): prime the link, then toggle
        Bluetooth OFF→ON on P2 (`svc bluetooth disable/enable` — exactly what a
        user does, or airplane mode). Every scan/advertise/GATT handle P2 held
        is now dead. Pass: P2's `btStateReceiver` drops the stale handles,
        replays advertise + scan, the core follows (`ble.adapter_off` →
        `ble.adapter_recovered`), and a fresh P1→P2 message delivers again
        within 60 s. 3 cycles."""
        import time
        _CYCLES = 3
        _BUDGET_S = 60

        for ad in self.ads:
            ad.adb.logcat(["-c"])
        r = self.send_and_await(self.p1, self.p2, "prime", timeout_ms=_DELIVER_TIMEOUT_MS)
        asserts.assert_true(r.get("delivered"), f"link must be up first; {r}")

        recoveries = []
        for i in range(_CYCLES):
            self.p2.adb.shell(["svc", "bluetooth", "disable"])
            time.sleep(3)
            self.p2.adb.shell(["svc", "bluetooth", "enable"])
            # wait for the adapter to actually come back before timing recovery
            for _ in range(20):
                state = self.p2.adb.shell(
                    ["settings", "get", "global", "bluetooth_on"]
                ).decode("utf-8", "replace").strip()
                if state == "1":
                    break
                time.sleep(1)
            t0 = time.monotonic()
            ok = False
            while time.monotonic() - t0 < _BUDGET_S:
                rr = self.send_and_await(self.p1, self.p2, f"bt-toggle-{i}", timeout_ms=8000)
                if rr.get("delivered"):
                    ok = True
                    break
                time.sleep(2)
            dt = round(time.monotonic() - t0, 1)
            recoveries.append(dt)
            self.p2.log.info("bt-toggle %d/%d: recovered=%s in %ss", i + 1, _CYCLES, ok, dt)
            asserts.assert_true(ok, f"cycle {i}: no recovery within {_BUDGET_S}s")

        self.capture_evidence("tier1_bt_toggle")
        p2log = self._logcat_since_cleared(self.p2)
        # HV-31: P2's btStateReceiver must observe both toggle edges and the
        # core must follow the transition (drain_adapter_events).
        asserts.assert_true(
            "Bluetooth OFF" in p2log and "Bluetooth ON" in p2log,
            "btStateReceiver must observe both edges of every toggle",
        )
        asserts.assert_true(
            "ble.adapter_off" in p2log and "ble.adapter_recovered" in p2log,
            "the core must follow the toggle (ble.adapter_off / ble.adapter_recovered)",
        )
        # HV-10: no advertising error should have been left unretried.
        asserts.assert_true(
            "advertising retries exhausted" not in p2log,
            "advertising must recover within the bounded retry budget",
        )
        failed = p2log.count("msg.delivery_failed")
        self.p2.log.info("BT-TOGGLE: recoveries=%s delivery_failed=%d", recoveries, failed)

    # ---- HV-29: stopMesh/startMesh cycle on a live @Singleton engine ----

    def test_reconnect_mesh_cycle(self):
        """HV-29: `reconnectMesh()` (the RETRY button) does stopMesh()+startMesh()
        on the SAME process / same @Singleton IrisEngine — stop_all() aborts the
        engine's delivery/ack/gc loops and start_all() must bring them back
        (`MessageEngine::restart()`). Distinct from
        test_peer_process_restart_recovers, which force-stops the process (fresh
        engine). Cycle P2 3x; each time a P1->P2 message must deliver again
        within 45 s (a full mesh stop/start cycle: engine.shutdown -> restart,
        radios torn down + re-advertised, peer re-discovered + reconnected).
        Pass: 3/3, 0 msg.delivery_failed."""
        import time
        _CYCLES = 3
        _BUDGET_S = 45

        for ad in self.ads:
            ad.adb.logcat(["-c"])
        r = self.send_and_await(self.p1, self.p2, "prime", timeout_ms=_DELIVER_TIMEOUT_MS)
        asserts.assert_true(r.get("delivered"), f"link must be up first; {r}")

        recoveries = []
        for i in range(_CYCLES):
            self.p2.iris.stopMesh()
            time.sleep(2)
            self.p2.iris.startMesh()
            # keys are in-process (engine reused) — but re-register defensively.
            self.p1.iris.registerPeerKey(self.p2.iris.nodeId(), self.p2.iris.staticX25519())
            self.p2.iris.registerPeerKey(self.p1.iris.nodeId(), self.p1.iris.staticX25519())
            t0 = time.monotonic()
            ok = False
            while time.monotonic() - t0 < _BUDGET_S:
                rr = self.send_and_await(self.p1, self.p2, f"retry-{i}", timeout_ms=6000)
                if rr.get("delivered"):
                    ok = True
                    break
                time.sleep(2)
            dt = round(time.monotonic() - t0, 1)
            recoveries.append(dt)
            self.p1.log.info("retry-cycle %d/%d: recovered=%s in %ss", i + 1, _CYCLES, ok, dt)
            asserts.assert_true(ok, f"cycle {i}: RETRY did not revive the mesh within {_BUDGET_S}s")

        self.capture_evidence("tier3_reconnect_mesh")
        p2log = self._logcat_since_cleared(self.p2)
        self.p1.log.info("RECONNECT-MESH: recoveries=%s", recoveries)
        asserts.assert_equal(p2log.count("msg.delivery_failed"), 0,
                             "no message may be permanently failed across a RETRY")

    # ---- HV-94: zombie GATT link after the peer's app process restarts ----

    def test_peer_process_restart_recovers(self):
        """HV-94: prime P1->P2, then `am force-stop` P2's engine process (an
        OOM-kill or a swipe-away). P1's ACL/GATT connection and poller are
        untouched — P1 must notice the peer is gone (a failed write / a real
        disconnect) and reconnect to P2's fresh process. Pass: a P1->P2 message
        delivers again within 30 s of P2 coming back. 3 cycles.

        Distinct from HV-15 (both sides see the drop) and HV-31 (Bluetooth
        toggled) — here only P2's *app* dies, the radio stays up."""
        import time
        _SNIPPET_PKG = "org.iris.mesh.test"
        _CYCLES = 3
        _BUDGET_S = 30

        for ad in self.ads:
            ad.adb.logcat(["-c"])
        r = self.send_and_await(self.p1, self.p2, "prime", timeout_ms=_DELIVER_TIMEOUT_MS)
        asserts.assert_true(r.get("delivered"), f"link must be up first; {r}")

        recoveries = []
        for i in range(_CYCLES):
            # Kill P2's engine process (severs the Mobly snippet RPC too).
            self.p2.adb.shell(["am", "force-stop", _SNIPPET_PKG])
            time.sleep(2)
            # Relaunch the snippet + engine; P2's X25519 key is regenerated
            # (in-memory) so re-register it on P1 (nodeId persists).
            try:
                self.p2.unload_snippet("iris")
            except Exception:
                pass
            self.p2.load_snippet("iris", _SNIPPET_PKG)
            self.p2.iris.startMesh()
            self.p1.iris.registerPeerKey(self.p2.iris.nodeId(), self.p2.iris.staticX25519())
            self.p2.iris.registerPeerKey(self.p1.iris.nodeId(), self.p1.iris.staticX25519())

            t0 = time.monotonic()
            ok = False
            while time.monotonic() - t0 < _BUDGET_S:
                rr = self.send_and_await(self.p1, self.p2, f"restart-{i}", timeout_ms=6000)
                if rr.get("delivered"):
                    ok = True
                    break
                time.sleep(2)
            dt = round(time.monotonic() - t0, 1)
            recoveries.append(dt)
            self.p1.log.info("restart %d/%d: recovered=%s in %ss", i + 1, _CYCLES, ok, dt)
            asserts.assert_true(ok, f"cycle {i}: P1 did not recover to P2's new process within {_BUDGET_S}s")

        self.capture_evidence("tier1_peer_restart")
        self.p1.log.info("PEER-RESTART: recoveries=%s", recoveries)
        p1log = self._logcat_since_cleared(self.p1)
        asserts.assert_equal(
            p1log.count("msg.delivery_failed"), 0,
            "no message may be permanently failed while P1 recovers the link",
        )


if __name__ == "__main__":
    test_runner.main()
