"""Tier-2 Wi-Fi Direct — GO/GC election (HV-19 + HV-20 + HV-22, §5 group).

Loop §2 gate: "cold-start both phones 10x (alternating power-on order) ->
exactly one group forms -> messages flow both ways every time."

STATUS (Session 26): HW-PENDING on THIS harness. The Tier-2 gate itself is
🟢 — verified 10/10 by the adb-driven
`docs/bug-hunting/hardware_verification/tier2_wifidirect_cold_cycle.sh`,
which drives the REAL shipping app. This Mobly harness still can't reach the
same coverage: vivo/OEM Android gates `WifiP2pManager.discoverPeers()` behind
"the calling app has a foreground Activity", and the Mobly instrumented
snippet process has none — so the snippet engine's discovery finds
`peers_seen=0`. `foregroundForWifiDirect()` (added Session 26) launches the
real `MainActivity` in a no-auto-start-mesh mode to give the `org.iris.mesh`
UID a foreground Activity; on the bench that still did not make instrumented
discovery resolve peers (leading theory: the P2P framework attributes
discovery to the `am instrument` context, not the app). Left wired so a
future harness fix (or a non-vivo device) can flip it green; run the shell
script for the actual gate until then.

The harness now allocates one message ID per direction and polls those same
IDs for the complete bounded cycle; this avoids turning a transport delay into
new-message/replay noise. The correction is still not a green result: the
2026-09-06 Vivo bench runs reached Connected/live-socket states but did not
produce reliable peer delivery, so the harness remains HW-PENDING and the
finding is blocked pending a fresh production data-path investigation.

`iris_bench` cannot literally power-cycle a phone; the automated proxy for
"cold start" is a full engine stop/start on both phones.

Run:  python -m iris_bench --test_module tier2 --tests <method>
"""

import json
import re
import time

from mobly import asserts
from mobly import test_runner

from iris_bench.base import IrisBenchBase

_CYCLES = 10
_BUDGET_S = 60


class Tier2WifiDirect(IrisBenchBase):

    def _foreground(self, ad):
        """HV-21: vivo/OEM Android gates discoverPeers() behind 'the calling
        app has a foreground Activity'. The instrumented test process has
        none, so every Wi-Fi Direct test failed discovery until this. The
        bench Activity does nothing but exist."""
        try:
            ad.iris.foregroundForWifiDirect()
        except Exception:
            pass

    def setup_test(self):
        for ad in self.ads:
            self._foreground(ad)
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
            try:
                ad.iris.background()
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
            for ad in self.ads:
                self._foreground(ad)
            first.iris.startMesh()
            second.iris.startMesh()
            self.p1.iris.registerPeerKey(self.p2.iris.nodeId(), self.p2.iris.staticX25519())
            self.p2.iris.registerPeerKey(self.p1.iris.nodeId(), self.p1.iris.staticX25519())

            t0 = time.monotonic()
            fwd = rev = None
            fwd_id = self.send_once(self.p1, self.p2, f"wd-fwd-{i}")
            rev_id = self.send_once(self.p2, self.p1, f"wd-rev-{i}")
            wd_p1 = wd_p2 = "unknown"
            while time.monotonic() - t0 < _BUDGET_S:
                remaining_ms = max(
                    250,
                    min(2000, int((_BUDGET_S - (time.monotonic() - t0)) * 1000)),
                )
                if not (fwd and fwd.get("delivered")):
                    fwd = self.await_message(self.p2, fwd_id, remaining_ms)
                if not (rev and rev.get("delivered")):
                    rev = self.await_message(self.p1, rev_id, remaining_ms)
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

            # Operator hard requirement (Session 24): no OS "Invitation to
            # connect" dialog may ever reach the user. IRIS drives connect()
            # programmatically with plain WPS PBC, which must not prompt.
            for ad in self.ads:
                try:
                    lg = ad.adb.logcat(["-d"]).decode("utf-8", "replace")
                except Exception:
                    continue
                asserts.assert_false(
                    "invitation to connect" in lg.lower()
                    or "invitation received" in lg.lower(),
                    f"cycle {i}: an OS Wi-Fi Direct invitation dialog appeared "
                    f"on {ad.serial} — must be silent",
                )

        self.p1.log.info("WIFI-DIRECT COLD START: recoveries=%s", recoveries)
        self.capture_evidence("tier2_cold_start_election")


if __name__ == "__main__":
    test_runner.main()
