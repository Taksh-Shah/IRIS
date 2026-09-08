package iriscore.util

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.content.ContextCompat

/**
 * Runtime-permission set required before any IRIS transport can do useful work.
 *
 * The manifest declares every permission, but on API 31+ the Bluetooth ones and
 * on API 33+ `NEARBY_WIFI_DEVICES` are *runtime* grants: without them
 * `startScan`/`startAdvertising`/`connectGatt` and the Wi-Fi Aware/Direct
 * discovery calls fail silently (empty results or `SecurityException` swallowed
 * by the adapters' `runCatching`), so the mesh appears to run while carrying no
 * traffic at all. The shell must therefore request them before bring-up.
 *
 * Split by SDK level:
 *  - API <= 30: legacy `BLUETOOTH`/`BLUETOOTH_ADMIN` are install-time; BLE scan
 *    results still require `ACCESS_FINE_LOCATION` at runtime.
 *  - API 31+: `BLUETOOTH_SCAN` (declared `neverForLocation`),
 *    `BLUETOOTH_ADVERTISE`, `BLUETOOTH_CONNECT`.
 *  - API 33+: `NEARBY_WIFI_DEVICES` for Wi-Fi Aware/Direct discovery.
 *
 * NOTE: `POST_NOTIFICATIONS` is intentionally excluded. Android does not
 * require it to start a foreground service — the FGS system notification is
 * shown regardless. Gating the entire mesh on notification permission violates
 * Android policy and hides the app's core function from users who decline.
 * POST_NOTIFICATIONS is requested separately by the notification channel setup.
 */
object MeshPermissions {

    /** Permissions to request at runtime on the current device. */
    val required: List<String> = buildList {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            add(Manifest.permission.BLUETOOTH_SCAN)
            add(Manifest.permission.BLUETOOTH_ADVERTISE)
            add(Manifest.permission.BLUETOOTH_CONNECT)
        } else {
            // Pre-31 BLE scanning is gated on location, not on a BT runtime grant.
            add(Manifest.permission.ACCESS_FINE_LOCATION)
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            add(Manifest.permission.NEARBY_WIFI_DEVICES)
        }
    }

    /** Notification permission — requested separately, never gates the mesh. */
    val notificationPermission: String? =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU)
            Manifest.permission.POST_NOTIFICATIONS
        else null

    /** True once every entry in [required] is granted. */
    fun allGranted(context: Context): Boolean =
        required.all { granted(context, it) }

    /** The subset of [required] still missing — what the UI should ask for. */
    fun missing(context: Context): List<String> =
        required.filterNot { granted(context, it) }

    private fun granted(context: Context, permission: String): Boolean =
        ContextCompat.checkSelfPermission(context, permission) ==
            PackageManager.PERMISSION_GRANTED
}
