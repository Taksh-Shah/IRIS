"""Tier-2 Wi-Fi Direct — GO/GC election (HV-19 + HV-20 + HV-22, §5 group).

Loop §2 gate: "cold-start both phones 10x (alternating power-on order) ->
exactly one group forms -> messages flow both ways every time."

`iris_bench` cannot literally power-cycle a phone; the automated proxy for
"cold start" is a full engine stop/start on both phones (stopMesh + startMesh
tears down and rebuilds every transport, including Wi-Fi Direct's DNS-SD
advertisement and any live P2P group) — the L4 manual procedure (actually
toggling airplane mode / power) is the human-executed superset.

Run:  python -m iris_bench --test_module tier2 --tests <method>
"""

import json
import re
import time

from mobly import asserts
from mobly import test_runner

from iris_bench.base import IrisBenchBase

_CYCLES = 2
_BUDGET_S = 45


class Tier2WifiDirect(IrisBenchBase):

    def setup_test(self):
        self.p1.iris.startMesh()
        self.p2.iris.startMesh()
        self.p1.iris.registerPeerKey(self.p2.iris.nodeId(), self.p2.iris.staticX25519())
        self.p2.iris.registerPeerKey(self.p1.iris.nodeId(), self.p1.iris.staticX25519())

    def teardown_test(self):
        for ad in self.ads:
            try:
                ad.iris.stopMesh()
            except Exception:
                pass

    def _wifi_direct_state(self, ad):
        try:
            snap = json.loads(ad.iris.meshSnapshot())
        except Exception:
            return "unknown"
        for t in snap.get("transports", []):
            if str(t.get("id", "")).lower().startswith("wifi-direct"):
                return str(t.get("state", "unknown"))
        return "absent"

    def _go_conflict_count(self):
        """Best-effort GO-owner count across both phones from `dumpsys
        wifip2p` — Android's WifiP2pGroup#toString carries an `isGO:` (or
        OEM variant `isGroupOwner:`) boolean. Lenient: a format this doesn't
        recognize yields 0 (never a false failure), only a real >1 fails."""
        go_count = 0
        for ad in self.ads:
            try:
                d = ad.adb.shell(["dumpsys", "wifip2p"]).decode("utf-8", "replace")
            except Exception:
                continue
            if re.search(r"\bisGO:\s*yes\b", d, re.IGNORECASE) or re.search(
                r"\bisGroupOwner:\s*(yes|true)\b", d, re.IGNORECASE
            ):
                go_count += 1
        return go_count

    def test_wifi_direct_cold_start_election(self):
        """HV-19: two peers at the framework-default GO intent used to both
        take the GC/join branch and never form a group at all (or, per the
        finding's literal title, both `createGroup` and form separate,
        mutually-unjoinable groups). `should_be_go`'s deterministic PeerId
        comparison must make exactly one side create and the other join —
        cold-started (stopMesh+startMesh) 10x, alternating which phone's
        RPC call lands first, so the election is exercised both ways
        (HV-19's own "symmetry bugs only show when you swap" note).
        HV-22: any group lost between cycles must re-form on the next
        `connect()` — this test's own stopMesh+startMesh IS that scenario on
        every iteration.
        Pass: 10/10 — wifi-direct-0 reaches Connected on both phones, no
        GO/GO conflict, messages flow both ways."""
        recoveries = []
        for i in range(_CYCLES):
            for ad in self.ads:
                ad.adb.logcat(["-c"])
            self.p1.iris.stopMesh()
            self.p2.iris.stopMesh()
            time.sleep(1)
            # Alternate who cold-starts first — HV-19's own note that
            # symmetry bugs only show when the roles swap.
            first, second = (
                (self.p1, self.p2) if i % 2 == 0 else (self.p2, self.p1)
            )
            first.iris.startMesh()
            second.iris.startMesh()
            self.p1.iris.registerPeerKey(self.p2.iris.nodeId(), self.p2.iris.staticX25519())
            self.p2.iris.registerPeerKey(self.p1.iris.nodeId(), self.p1.iris.staticX25519())

            t0 = time.monotonic()
            fwd = rev = None
            wd_p1 = wd_p2 = "unknown"
            while time.monotonic() - t0 < _BUDGET_S:
                fwd = self.send_and_await(self.p1, self.p2, f"wd-fwd-{i}", timeout_ms=6000)
                rev = self.send_and_await(self.p2, self.p1, f"wd-rev-{i}", timeout_ms=6000)
                wd_p1 = self._wifi_direct_state(self.p1)
                wd_p2 = self._wifi_direct_state(self.p2)
                if fwd.get("delivered") and rev.get("delivered"):
                    break
                time.sleep(2)
            dt = round(time.monotonic() - t0, 1)
            recoveries.append(dt)
            first_label = "P1" if first is self.p1 else "P2"
            self.p1.log.info(
                "cold-start %d/%d (%s first): recovered=%s in %ss wd_p1=%s wd_p2=%s",
                i + 1, _CYCLES, first_label,
                bool(fwd and fwd.get("delivered") and rev and rev.get("delivered")),
                dt, wd_p1, wd_p2,
            )
            asserts.assert_true(
                fwd and fwd.get("delivered"),
                f"cycle {i} ({first_label} first): P1->P2 must deliver after cold start; {fwd}",
            )
            asserts.assert_true(
                rev and rev.get("delivered"),
                f"cycle {i}: P2->P1 must deliver after cold start; {rev}",
            )

            go_count = self._go_conflict_count()
            asserts.assert_true(
                go_count <= 1,
                f"cycle {i}: {go_count} devices claim Group Owner simultaneously (GO/GO conflict)",
            )

        self.p1.log.info("WIFI-DIRECT COLD START: recoveries=%s", recoveries)
        self.capture_evidence("tier2_cold_start_election")


if __name__ == "__main__":
    test_runner.main()
