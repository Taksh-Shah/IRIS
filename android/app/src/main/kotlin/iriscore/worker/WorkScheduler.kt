package iriscore.worker

import android.content.Context
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit

/**
 * AC-7 — WorkManager scheduling. Unique periodic work, 15-min cadence with a
 * 5-min flex for wake coalescing; network-connected constraint only (relay
 * checks ride the INTERNET-001 transport when present — never flaps the radio
 * for the check itself).
 */
object WorkScheduler {

    const val UNIQUE_PERIODIC = "iris_mesh_sync"

    fun schedule(context: Context) {
        val request = PeriodicWorkRequestBuilder<IrisBackgroundSyncWorker>(
            MeshSyncPolicy.CADENCE_MINUTES,
            TimeUnit.MINUTES,
            MeshSyncPolicy.CADENCE_FLEX_MINUTES,
            TimeUnit.MINUTES,
        )
            .setConstraints(
                Constraints.Builder()
                    .setRequiredNetworkType(NetworkType.CONNECTED)
                    .build(),
            )
            .build()

        WorkManager.getInstance(context).enqueueUniquePeriodicWork(
            UNIQUE_PERIODIC,
            ExistingPeriodicWorkPolicy.UPDATE,
            request,
        )
    }

    fun cancel(context: Context) {
        WorkManager.getInstance(context).cancelUniqueWork(UNIQUE_PERIODIC)
    }
}