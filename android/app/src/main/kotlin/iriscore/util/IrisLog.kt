package iriscore.util

import android.util.Log

/**
 * HV-86: one logcat tag (`iriscore`) and one `iris.<area>` message prefix for
 * every Kotlin-adapter diagnostic, matching the Rust `tracing` events that
 * already land under that tag. A bench operator runs a single
 * `adb logcat -s iriscore` and filters by `iris.ble` / `iris.wd` / … instead of
 * juggling `IrisBle` / `IrisBleDiag` / `IrisWifiDirectDiag`.
 *
 * `area` is a dotted path — `"ble.scan"`, `"ble.gatt"`, `"wd.dnssd"`,
 * `"wd.init"` — kept short and consistent with the Rust taxonomy
 * (`discovery.*`, `msg.*`, `engine.*`).
 */
object IrisLog {
    private const val TAG = "iriscore"

    fun d(area: String, msg: String) = Log.d(TAG, "iris.$area $msg")

    fun w(area: String, msg: String, t: Throwable? = null) {
        if (t != null) Log.w(TAG, "iris.$area $msg", t) else Log.w(TAG, "iris.$area $msg")
    }
}
