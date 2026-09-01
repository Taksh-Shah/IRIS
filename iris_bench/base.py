"""IrisBenchBase — the shared Mobly fixture for every tier's test module.

`setup_class` registers both phones as Android controllers, force-installs the
current app + snippet APKs, grants the runtime permissions the engine needs,
loads the `IrisSnippet` RPC surface on each, and starts a per-test evidence
capture. Subclasses get `self.p1` / `self.p2` (label-sorted) and helpers.
"""

import os
import time

from mobly import base_test
from mobly import utils
from mobly.controllers import android_device

from iris_bench import evidence

# Runtime permissions the engine's transports need. Missing ones on older API
# levels are ignored (the shell 'grant' just errors and we swallow it).
_PERMISSIONS = [
    "android.permission.BLUETOOTH_SCAN",
    "android.permission.BLUETOOTH_CONNECT",
    "android.permission.BLUETOOTH_ADVERTISE",
    "android.permission.ACCESS_FINE_LOCATION",
    "android.permission.NEARBY_WIFI_DEVICES",
    "android.permission.POST_NOTIFICATIONS",
]

_APP_PKG = "org.iris.mesh"
_SNIPPET_PKG = "org.iris.mesh.test"

_REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
_APP_APK = os.path.join(
    _REPO_ROOT, "android/app/build/outputs/apk/debug/app-debug.apk"
)
_SNIPPET_APK = os.path.join(
    _REPO_ROOT,
    "android/app/build/outputs/apk/androidTest/debug/app-debug-androidTest.apk",
)


class IrisBenchBase(base_test.BaseTestClass):

    def setup_class(self):
        self.ads = self.register_controller(android_device, min_number=2)
        # `ad.load_config` set `label` from the config; fall back to serial.
        for ad in self.ads:
            if not getattr(ad, "label", None):
                ad.label = ad.serial
        self.ads.sort(key=lambda ad: ad.label)
        self.p1, self.p2 = self.ads[0], self.ads[1]

        for ad in self.ads:
            self._prepare_device(ad)

        self.session_dir = self._make_session_dir()
        for ad in self.ads:
            ad.adb.logcat(["-c"])  # clear before the run

    def _prepare_device(self, ad):
        if str(self.user_params.get("install", "true")).lower() != "false":
            for apk in (_APP_APK, _SNIPPET_APK):
                if not os.path.isfile(apk):
                    raise RuntimeError(
                        f"missing APK: {apk}\n"
                        "build first: cd android && java -cp gradle/wrapper/"
                        "gradle-wrapper.jar org.gradle.wrapper.GradleWrapperMain "
                        ":app:assembleDebug :app:assembleDebugAndroidTest"
                    )
            ad.adb.install(["-r", "-d", "-g", _APP_APK])
            ad.adb.install(["-r", "-d", _SNIPPET_APK])
        for perm in _PERMISSIONS:
            try:
                ad.adb.shell(["pm", "grant", _APP_PKG, perm])
            except Exception:
                pass  # permission not defined on this API level
        # Battery-optimisation exemption so the engine's loops are not throttled.
        try:
            ad.adb.shell(["dumpsys", "deviceidle", "whitelist", "+" + _APP_PKG])
        except Exception:
            pass
        ad.load_snippet("iris", _SNIPPET_PKG)
        ad.log.info("snippet loaded; node=%s", ad.iris.nodeId())

    def _make_session_dir(self):
        base = os.path.join(
            _REPO_ROOT,
            "docs/bug-hunting/hardware_verification/evidence",
        )
        os.makedirs(base, exist_ok=True)
        existing = [d for d in os.listdir(base) if d.startswith("session-")]
        n = 1 + max((int(d.split("-")[1]) for d in existing if d.split("-")[1].isdigit()),
                    default=0)
        d = os.path.join(base, f"session-{n:02d}")
        os.makedirs(d, exist_ok=True)
        return d

    # ---- helpers used by the tier test modules -----------------------------

    def start_mesh_both(self):
        """Start the engine on both phones and wire the key directories (HV-89
        interim: hand the each other's X25519 static key)."""
        self.p1.iris.startMesh()
        self.p2.iris.startMesh()
        self.p1.iris.registerPeerKey(self.p2.iris.nodeId(), self.p2.iris.staticX25519())
        self.p2.iris.registerPeerKey(self.p1.iris.nodeId(), self.p1.iris.staticX25519())

    def stop_mesh_both(self):
        for ad in self.ads:
            try:
                ad.iris.stopMesh()
            except Exception:
                pass

    def send_and_await(self, src, dst, text, priority=4, timeout_ms=60000):
        """src sends `text` to dst; returns dst's awaitDelivered JSON dict.

        `latencyMs` is measured host-side (send RPC return -> deliver RPC return)
        because the two snippets are separate processes and can't share a clock.
        """
        import json
        import time

        t0 = time.monotonic()
        msg_id = src.iris.sendText(dst.iris.nodeId(), text, priority)
        result = json.loads(dst.iris.awaitDelivered(msg_id, timeout_ms))
        result["messageId"] = msg_id
        if result.get("delivered"):
            result["latencyMs"] = round((time.monotonic() - t0) * 1000)
        src.log.info("sent id=%s -> %s : %s", msg_id, dst.label, result)
        return result

    def capture_evidence(self, tag):
        evidence.capture(self.ads, self.session_dir, tag)

    def on_fail(self, record):
        if not getattr(self, "ads", None):
            return
        try:
            self.capture_evidence(f"FAIL-{record.test_name}")
        except Exception as e:
            for ad in self.ads:
                ad.log.warning("evidence capture failed: %s", e)

    def teardown_class(self):
        if not getattr(self, "ads", None):
            return
        self.stop_mesh_both()
        for ad in self.ads:
            try:
                ad.unload_snippet("iris")
            except Exception:
                pass
