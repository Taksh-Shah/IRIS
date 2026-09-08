package iriscore.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import androidx.core.content.ContextCompat
import dagger.hilt.android.AndroidEntryPoint
import iriscore.R
import iriscore.data.MeshRepository
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import javax.inject.Inject

/**
 * AC-7 — `connectedDevice` foreground service (API 34+).
 *
 * Raises the app to a "currently using a connected device" foreground type so
 * the BLE control-plane trigger + Wi-Fi Direct re-arm window + Wi-Fi Aware
 * production discovery can run while the app is backgrounded.
 *
 * WP3: the mesh engine lifecycle is owned here, not by the ViewModel. This
 * means the engine keeps running through screen rotations, configuration
 * changes, and any number of ViewModel recreations. [MeshRepository.startMesh]
 * is called in [onStartCommand]; [MeshRepository.stopMesh] is called when the
 * operator explicitly stops the service via [ACTION_STOP] (user-initiated) or
 * when Android kills and does not restart the service. START_STICKY ensures the
 * service — and the engine — restart automatically after an OS kill.
 */
@AndroidEntryPoint
class IrisBleService : Service() {

    @Inject
    lateinit var repository: MeshRepository

    private var scanSession: BleScanSession? = null

    /**
     * Fire-and-forget coroutine scope for the blocking FFI calls
     * ([MeshRepository.startMesh], [MeshRepository.stopMesh]). Not cancelled in
     * [onDestroy] so that a teardown dispatched on the last line of [onDestroy]
     * can complete even after the Android runtime considers the service dead.
     */
    private val serviceScope = CoroutineScope(
        SupervisorJob() +
            Dispatchers.Default +
            CoroutineExceptionHandler { _, _ -> /* mesh errors are logged inside the repository */ },
    )

    override fun onCreate() {
        super.onCreate()
        createChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                isRunning = false
                serviceScope.launch { repository.stopMesh() }
                stopSelf()
                return START_NOT_STICKY
            }
        }
        startForegroundCompat()
        isRunning = true
        if (scanSession == null) {
            BleScanSession(this).also {
                it.start()
                scanSession = it
            }
        }
        // WP3: bring the Rust engine up here so it outlives any single ViewModel
        // instance. subscribeInbox is idempotent (AtomicBoolean guard inside the
        // repository) so repeated onStartCommand calls from START_STICKY restarts
        // or quick stop+start cycles (reconnect) are safe.
        serviceScope.launch {
            repository.startMesh()
            repository.subscribeInbox()
        }
        // WP9: ensure the message notification channel exists, then watch the
        // uiState flow for new inbound messages and post a notification for each.
        MessageNotificationHelper.createChannel(this)
        serviceScope.launch {
            var knownCount = repository.uiState.value.messages.count { !it.isOutbound }
            repository.uiState.collect { state ->
                val inboundCount = state.messages.count { !it.isOutbound }
                if (inboundCount > knownCount) {
                    val newest = state.messages.lastOrNull { !it.isOutbound }
                    if (newest != null) {
                        MessageNotificationHelper.postMessageNotification(
                            this@IrisBleService,
                            newest.senderId,
                            newest.payloadUtf8.take(80),
                        )
                    }
                    knownCount = inboundCount
                }
            }
        }
        return START_STICKY
    }

    override fun onDestroy() {
        isRunning = false
        scanSession?.stop()
        scanSession = null
        super.onDestroy()
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun createChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                getString(R.string.service_channel_name),
                NotificationManager.IMPORTANCE_LOW,
            ).apply { description = getString(R.string.service_channel_description) }
            getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
    }

    private fun buildNotification(): Notification =
        NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle(getString(R.string.service_notification_title))
            .setContentText(getString(R.string.service_notification_text))
            .setSmallIcon(R.drawable.ic_stat_iris)
            .setOngoing(true)
            .setSilent(true)
            .build()

    private fun startForegroundCompat() {
        val notification = buildNotification()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.UPSIDE_DOWN_CAKE) {
            startForeground(
                NOTIFICATION_ID,
                notification,
                ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE,
            )
        } else {
            startForeground(NOTIFICATION_ID, notification)
        }
    }

    companion object {
        const val CHANNEL_ID = "iris_mesh_links"
        private const val NOTIFICATION_ID = 1
        const val ACTION_STOP = "iriscore.service.STOP"

        /**
         * HV-32: `true` between `onStartCommand` and `onDestroy`. The mesh
         * engine + radios are only alive while this foreground service is —
         * `IrisBackgroundSyncWorker` reads this to avoid building a SECOND
         * `@Singleton IrisEngine` in a bare background process (new tokio
         * runtime, no FGS, radios can't come up) just to drain the relay
         * outbox. Process-local (a fresh process starts `false`), which is
         * exactly the signal we want.
         */
        @Volatile
        var isRunning: Boolean = false
            private set

        fun start(context: Context) {
            val intent = Intent(context, IrisBleService::class.java)
            ContextCompat.startForegroundService(context, intent)
        }

        fun stop(context: Context) {
            val intent = Intent(context, IrisBleService::class.java).setAction(ACTION_STOP)
            context.stopService(intent)
        }
    }
}
