package iriscore.service

import android.bluetooth.le.BluetoothLeScanner
import android.bluetooth.le.ScanResult
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent

/**
 * PendingIntent scan delivery receiver (AC-7, API 26+). The platform sends the
 * scan intent here; results are forwarded to [BleScanEvents] where the control
 * plane trigger is consumed. Explicitly non-exported.
 */
class BleScanReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_SCAN_RESULTS) return

        // PendingIntent scans are API 26+ only, which is also this module's
        // minSdk — so the list extra is always the delivery path (there is no
        // pre-O branch to fall back to). The typed getParcelableArrayListExtra
        // overload is API 33+; the deprecated one is the only option at 26.
        @Suppress("DEPRECATION")
        val results: List<ScanResult> =
            intent.getParcelableArrayListExtra<ScanResult>(
                BluetoothLeScanner.EXTRA_LIST_SCAN_RESULT,
            ) ?: emptyList()

        for (result in results) {
            BleScanEvents.onScanResult(result)
        }
    }

    companion object {
        const val ACTION_SCAN_RESULTS = "iriscore.service.SCAN_RESULTS"
    }
}