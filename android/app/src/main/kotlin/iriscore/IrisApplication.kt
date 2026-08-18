package iriscore

import android.app.Application
import androidx.hilt.work.HiltWorkerFactory
import androidx.work.Configuration
import dagger.hilt.android.HiltAndroidApp
import javax.inject.Inject

/**
 * IRIS application root. Hilt graph + WorkManager Hilt WorkerFactory
 * (AC-7 `IrisBackgroundSyncWorker` dependency injection).
 */
@HiltAndroidApp
class IrisApplication : Application(), Configuration.Provider {

    @Inject
    lateinit var workerFactory: HiltWorkerFactory

    override val workManagerConfiguration: Configuration
        get() = Configuration.Builder()
            .setWorkerFactory(workerFactory)
            .build()
}