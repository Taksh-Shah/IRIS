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
 *    results still require `ACCESS_FINE_LOCATION` at runtime. Wi-Fi Direct's
 *    `discoverPeers()`/`connect()` also require it on these levels.
 *  - API 31+: `BLUETOOTH_SCAN` (declared `neverForLocation`),
 *    `BLUETOOTH_ADVERTISE`, `BLUETOOTH_CONNECT`.
 *  - API 31/32 specifically: `NEARBY_WIFI_DEVICES` does not exist yet, and
 *    the API-31+ Bluetooth grants above do not cover Wi-Fi Direct — so
 *    `ACCESS_FINE_LOCATION` is *also* required here, same as pre-31. Without
 *    it, Wi-Fi Direct discovery silently never works on Android 12/12L (the
 *    `SecurityException` is swallowed by the adapters' own `runCatching`).
 *  - API 33+: `NEARBY_WIFI_DEVICES` for Wi-Fi Aware/Direct discovery,
 *    replacing the location requirement from this level onward.
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
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            // Bug fix: API 31/32 (Android 12/12L) fell through both branches
            // above with neither location nor NEARBY_WIFI_DEVICES requested.
            // NEARBY_WIFI_DEVICES only exists from API 33 -- on 31/32,
            // WifiP2pManager.discoverPeers()/connect() still require
            // ACCESS_FINE_LOCATION, and the >= S branch above only requests
            // the Bluetooth runtime grants, not location. Without it every
            // Wi-Fi Direct discoverPeers()/connect() call on these two API
            // levels throws a SecurityException that the adapters' own
            // runCatching swallows -- Wi-Fi Direct silently never worked on
            // Android 12/12L.
            add(Manifest.permission.ACCESS_FINE_LOCATION)
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
