package iriscore.service

import android.bluetooth.BluetoothLeScanner
import android.bluetooth.le.ScanResult
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build

/**
 * PendingIntent scan delivery receiver (AC-7, API 26+). The platform sends the
 * scan intent here; results are forwarded to [BleScanEvents] where the control
 * plane trigger is consumed. Explicitly non-exported.
 */
class BleScanReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACTION_SCAN_RESULTS) return

        val results: List<ScanResult> = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            intent.getParcelableArrayListExtra(BluetoothLeScanner.EXTRA_SCAN_RESULT)
        } else {
            @Suppress("DEPRECATION")
            (intent.getSerializableExtra(BluetoothLeScanner.EXTRA_SCAN_RESULT) as? List<*>)
                ?.filterIsInstance<ScanResult>()
        } ?: emptyList()

        for (result in results) {
            BleScanEvents.onScanResult(result)
        }
    }

    companion object {
        const val ACTION_SCAN_RESULTS = "iriscore.service.SCAN_RESULTS"
    }
}