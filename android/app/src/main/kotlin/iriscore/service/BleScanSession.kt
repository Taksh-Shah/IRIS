package iriscore.service

import android.Manifest
import android.annotation.SuppressLint
import android.bluetooth.le.BluetoothLeScanner
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanSettings
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.bluetooth.le.ScanResult
import android.os.ParcelUuid
import androidx.core.content.ContextCompat
import iriscore.adapter.AndroidBleTransportAdapter
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * PendingIntent BLE scanning (AC-7): Oreo+ callback-free scans delivered to the
 * manifest receiver [BleScanReceiver] via a [android.app.PendingIntent] — the
 * battery-conscious scanning pattern (no long-lived scan callback object, the
 * platform delivers results while the app process is in the background).
 *
 * Cadence: [ScanRestartPolicy] window/backoff loop driven from a coroutine.
 * Discovered IRIS advertisements are forwarded to [BleScanEvents] (in-process
 * shared flow consumed by the shell for the BLE-control-plane trigger).
 */
class BleScanSession(private val context: Context) {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var job: Job? = null
    private var consecutiveEmpties = 0
    @Volatile private var activeWindow = false

    /** PendingIntent of the current scan window (same REQ_CODE + intent identity for stop). */
    private var scanPendingIntent: android.app.PendingIntent? = null

    /** @return `null` when the radio/permissions are unavailable. */
    @SuppressLint("MissingPermission")
    private val scanner: BluetoothLeScanner? =
        (context.getSystemService(Context.BLUETOOTH_SERVICE) as? android.bluetooth.BluetoothManager)
            ?.adapter
            ?.bluetoothLeScanner

    fun start(policy: ScanRestartPolicy = ScanRestartPolicy) {
        stop()
        val guarded = scanner ?: run {
            BleScanEvents.onUnavailable()
            return
        }
        job = scope.launch {
            while (isActive) {
                if (!hasPermission()) {
                    BleScanEvents.onUnavailable()
                    delay(policy.cooldownMs(consecutiveEmpties))
                    continue
                }
                activeWindow = true
                val found = startOneWindow(guarded)
                if (found) delay(ScanRestartPolicy.WINDOW_ON_MS)
                activeWindow = false
                stopScanWindow(guarded)
                if (!found) consecutiveEmpties = (consecutiveEmpties + 1)
                else consecutiveEmpties = 0
                delay(policy.offMs(consecutiveEmpties))
            }
        }
    }

    private fun hasPermission(): Boolean =
        ContextCompat.checkSelfPermission(
            context,
            Manifest.permission.BLUETOOTH_SCAN,
        ) == PackageManager.PERMISSION_GRANTED

    private fun startOneWindow(scanner: BluetoothLeScanner): Boolean {
        val settings = ScanSettings.Builder()
            .setScanMode(ScanSettings.SCAN_MODE_OPPORTUNISTIC)
            .build()
        val filters = listOf(
            ScanFilter.Builder()
                .setServiceUuid(ParcelUuid(AndroidBleTransportAdapter.IRIS_SERVICE_UUID))
                .build(),
        )
        return try {
            val pendingIntent = buildScanPendingIntent()
            scanner.startScan(filters, settings, pendingIntent)
            scanPendingIntent = pendingIntent
            true
        } catch (_: Exception) {
            false
        }
    }

    private fun stopScanWindow(scanner: BluetoothLeScanner) {
        val pendingIntent = scanPendingIntent ?: return
        scanPendingIntent = null
        try {
            scanner.stopScan(pendingIntent)
        } catch (_: Exception) {
            // Best-effort stop; a window that never started has nothing to stop.
        }
    }

    private fun buildScanPendingIntent(): android.app.PendingIntent {
        val intent = Intent(context, BleScanReceiver::class.java)
            .setAction(BleScanReceiver.ACTION_SCAN_RESULTS)
        return android.app.PendingIntent.getBroadcast(
            context,
            PendingIntentRequestCode,
            intent,
            android.app.PendingIntent.FLAG_UPDATE_CURRENT or android.app.PendingIntent.FLAG_IMMUTABLE,
        )
    }

    fun stop() {
        job?.cancel()
        job = null
        // AND-RT-106 re-review (R5): a window active at stop() must not be left
        // scanning — stop the PendingIntent window so no scan session leaks.
        scanner?.let { stopScanWindow(it) }
    }

    val isScanActive: Boolean get() = activeWindow

    companion object {
        /** Matches the manifest `<receiver>` request-code contract. */
        const val PendingIntentRequestCode = 0x1F1
    }
}

/**
 * In-process mDNS-style scanner event bus: `ScanResult`s surfaced by the
 * PendingIntent receiver are decoded + forwarded here for the shell.
 */
object BleScanEvents {

    data class Advert(val macHex: String, val rssi: Int)

    private val _adverts = MutableSharedFlow<Advert>(extraBufferCapacity = 64)
    val adverts = _adverts.asSharedFlow()

    private val _unavailable = MutableSharedFlow<Unit>(extraBufferCapacity = 1)
    val unavailable = _unavailable.asSharedFlow()

    fun onScanResult(result: ScanResult) {
        _adverts.tryEmit(
            Advert(
                macHex = result.device.address.replace(":", "").lowercase(),
                rssi = result.rssi,
            ),
        )
    }

    fun onUnavailable() {
        _unavailable.tryEmit(Unit)
    }
}