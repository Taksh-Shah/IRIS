package iriscore.worker

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import iriscore.data.MeshRepository
import iriscore.service.IrisBleService

/**
 * AC-7 — periodic store-and-forward relay check (15-min cadence, see
 * [MeshSyncPolicy]). Drains the app-side outbox into the Rust engine via
 * [MeshRepository.drainRelayOutbox] so queued P0/P1 texts are pushed as soon as
 * a transport is back up.
 *
 * Transient engine failures return [Result.retry] (backs off exponentially within
 * the WorkManager contract); success or a permanently-closed engine returns
 * [Result.success] (a broken engine must not wedge the periodic worker).
 *
 * HV-32: the worker must NOT touch `engine.get()` when the foreground service
 * (and thus the real engine + radios) is not alive. In a bare background process
 * that would build a SECOND `@Singleton IrisEngine` — new tokio runtime,
 * registered transports it cannot bring up without an FGS — and it would sit
 * there failing. The relay outbox is durable; it drains when the FGS next comes
 * up (`MeshViewModel.ensureStarted` / the RETRY path already call
 * `drainRelayOutbox`). Starting the FGS from the worker (Android-12+ expedited
 * job) is a possible follow-up, gated on a bench test.
 */
@HiltWorker
class IrisBackgroundSyncWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted workerParameters: WorkerParameters,
    private val repository: MeshRepository,
) : CoroutineWorker(appContext, workerParameters) {

    override suspend fun doWork(): Result {
        if (repository.pendingRelayCount == 0) return Result.success()
        // HV-32: no FGS => no live engine/radios in this process. Leave the
        // durable outbox alone; retry next window (cheap, no engine built).
        if (!IrisBleService.isRunning) return Result.retry()
        return try {
            val sent = repository.drainRelayOutbox()
            if (sent > 0) Result.success() else Result.retry()
        } catch (_: Exception) {
            // Constraint flapped or engine teardown raced the drain; retry next window.
            Result.retry()
        }
    }
}