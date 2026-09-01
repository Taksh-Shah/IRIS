"""HV-2 / HV-78 — per-test evidence capture.

For every run (and every failure) we pull, without anyone remembering to:
  - time-tagged `logcat` filtered to the iris tag + BT/Wi-Fi native
  - `dumpsys bluetooth_manager` and `dumpsys wifip2p`
  - the `meshSnapshot()` JSON from each phone
  - `btsnoop` HCI log via `adb bugreport` when the file is not directly
    readable (non-root vivo) — best-effort, large, skipped if it fails

Everything lands under
`docs/bug-hunting/hardware_verification/evidence/session-NN/<tag>/`.
"""

import os
import time
import zipfile


def _write(path, data):
    with open(path, "w", encoding="utf-8", errors="replace") as f:
        f.write(data)


def capture(ads, session_dir, tag):
    out = os.path.join(session_dir, tag.replace("/", "_"))
    os.makedirs(out, exist_ok=True)
    for ad in ads:
        label = getattr(ad, "label", ad.serial)
        prefix = os.path.join(out, label)

        try:
            log = ad.adb.logcat(["-d", "-v", "time"]).decode("utf-8", "replace")
            _write(prefix + ".logcat.txt", log)
        except Exception as e:
            _write(prefix + ".logcat.err", str(e))

        for svc in ("bluetooth_manager", "wifip2p"):
            try:
                d = ad.adb.shell(["dumpsys", svc]).decode("utf-8", "replace")
                _write(f"{prefix}.dumpsys.{svc}.txt", d)
            except Exception as e:
                _write(f"{prefix}.dumpsys.{svc}.err", str(e))

        try:
            snap = ad.iris.meshSnapshot()
            _write(prefix + ".meshSnapshot.json", snap)
        except Exception as e:
            _write(prefix + ".meshSnapshot.err", str(e))

    ad0 = ads[0]
    ad0.log.info("evidence -> %s", out)
    return out


def pull_btsnoop(ad, out_dir):
    """Best-effort btsnoop extraction via bugreport (non-root path). Slow."""
    label = getattr(ad, "label", ad.serial)
    br_zip = os.path.join(out_dir, f"{label}.bugreport.zip")
    try:
        ad.adb.bugreport([br_zip], timeout=240)
    except Exception as e:
        ad.log.warning("bugreport failed (%s) — no btsnoop this run", e)
        return None
    try:
        with zipfile.ZipFile(br_zip) as z:
            for name in z.namelist():
                if name.endswith("btsnoop_hci.log"):
                    dst = os.path.join(out_dir, f"{label}.btsnoop_hci.log")
                    with z.open(name) as src, open(dst, "wb") as f:
                        f.write(src.read())
                    ad.log.info("btsnoop -> %s", dst)
                    return dst
    except Exception as e:
        ad.log.warning("btsnoop extract failed: %s", e)
    return None
