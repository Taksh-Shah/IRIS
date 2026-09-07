package iriscore

import android.app.Application
import androidx.hilt.work.HiltWorkerFactory
import androidx.work.Configuration
import dagger.hilt.android.HiltAndroidApp
import iriscore.adapter.AndroidInternetNetworkMonitor
import iriscore.adapter.AndroidNsdAdapter
import javax.inject.Inject

/**
 * IRIS application root. Hilt graph + WorkManager Hilt WorkerFactory
 * (AC-7 `IrisBackgroundSyncWorker` dependency injection).
 */
@HiltAndroidApp
class IrisApplication : Application(), Configuration.Provider {

    @Inject
    lateinit var workerFactory: HiltWorkerFactory

    /** Process-lifetime owner for the Android validated-network callback. */
    @Inject
    lateinit var internetNetworkMonitor: AndroidInternetNetworkMonitor

    /** Process-lifetime owner for `_iris._tcp` NSD advertising + discovery. */
    @Inject
    lateinit var nsdAdapter: AndroidNsdAdapter

    override val workManagerConfiguration: Configuration
        get() = Configuration.Builder()
            .setWorkerFactory(workerFactory)
            .build()

    override fun onTerminate() {
        nsdAdapter.stop()
        internetNetworkMonitor.stop()
        super.onTerminate()
    }
}
