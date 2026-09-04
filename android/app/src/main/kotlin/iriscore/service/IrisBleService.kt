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
import iriscore.R

/**
 * AC-7 — `connectedDevice` foreground service (API 34+).
 *
 * Raises the app to a "currently using a connected device" foreground type so
 * the BLE control-plane trigger + Wi-Fi Direct re-arm window + Wi-Fi Aware
 * production discovery can run while the app is backgrounded. Started per
 * transport need from the shell; stopped via [ACTION_STOP].
 *
 * The `startForeground(..., FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE)` overload
 * is API 34+; on < 34 the plain 2-arg form is used (type folds into the legacy
 * foreground contract).
 */
class IrisBleService : Service() {

    private var scanSession: BleScanSession? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                isRunning = false
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