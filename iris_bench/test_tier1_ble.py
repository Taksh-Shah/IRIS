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


if __name__ == "__main__":
    test_runner.main()
