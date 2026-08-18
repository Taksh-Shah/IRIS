package iriscore.worker

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import iriscore.data.MeshRepository

/**
 * AC-7 — periodic store-and-forward relay check (15-min cadence, see
 * [MeshSyncPolicy]). Drains the app-side outbox into the Rust engine via
 * [MeshRepository.drainRelayOutbox] so queued P0/P1 texts are pushed as soon as
 * a transport is back up.
 *
 * Transient engine failures return [Result.retry] (backs off exponentially within
 * the WorkManager contract); success or a permanently-closed engine returns
 * [Result.success] (a broken engine must not wedge the periodic worker).
 */
@HiltWorker
class IrisBackgroundSyncWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted workerParameters: WorkerParameters,
    private val repository: MeshRepository,
) : CoroutineWorker(appContext, workerParameters) {

    override suspend fun doWork(): Result {
        if (repository.pendingRelayCount == 0) return Result.success()
        return try {
            val sent = repository.drainRelayOutbox()
            if (sent > 0) Result.success() else Result.retry()
        } catch (_: Exception) {
            // Constraint flapped or engine teardown raced the drain; retry next window.
            Result.retry()
        }
    }
}