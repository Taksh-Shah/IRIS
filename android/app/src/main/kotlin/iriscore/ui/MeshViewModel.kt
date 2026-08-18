package iriscore.ui

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import iriscore.data.MeshRepository
import iriscore.service.IrisBleService
import iriscore.ui.state.MeshUiState
import iriscore.worker.WorkScheduler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import javax.inject.Inject

/**
 * AC-6 MVVM state holder. Drives the engine bring-up on first composition and
 * exposes [MeshUiState] via [StateFlow] consumed with
 * [androidx.lifecycle.compose.collectAsStateWithLifecycle] (strong-skipping
 * safe: immutable state + stable keys).
 *
 * Boot path (AC-7): raises the connectedDevice FGS (BLE control-plane + Wi-Fi
 * re-arm window) and enqueues the 15-min WorkManager relay cadence.
 */
@HiltViewModel
class MeshViewModel @Inject constructor(
    @ApplicationContext private val appContext: Context,
    private val repository: MeshRepository,
) : ViewModel() {

    val uiState: StateFlow<MeshUiState> = repository.uiState

    // AND-RT-101: engine bring-up/teardown are blocking FFI calls — never on
    // the main thread. Serialized so start/stop cannot interleave.
    private val meshMutex = Mutex()

    // viewModelScope is cancelled before onCleared() runs, so teardown uses a
    // scope that survives until the engine actually stops.
    private val teardownScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    init {
        // Boot is best-effort: a missing runtime permission or unavailable radio
        // must not crash the shell surface (permission UX is TEST-stage scope).
        try {
            IrisBleService.start(appContext)
            WorkScheduler.schedule(appContext)
            viewModelScope.launch(Dispatchers.Default) {
                meshMutex.withLock {
                    repository.startMesh()
                    repository.subscribeInbox()
                }
            }
        } catch (_: RuntimeException) {
            // ForegroundServiceStartNotAllowedException / radio unavailable
            // Surface stays on IDLE/UNAVAILABLE until permissions are granted.
        }
    }

    fun send(recipientHex: String, text: String, priority: UByte = 0u) {
        if (recipientHex.isBlank() || text.isBlank()) return
        // engine.sendText is a blocking FFI call — never on the main thread.
        viewModelScope.launch { withContext(Dispatchers.IO) { repository.send(recipientHex, text, priority) } }
    }

    fun syncRelayNow() {
        // engine.sendText is a blocking FFI call — never on the main thread
        // (AND-RT-101/AND-RT-101-R3 re-review: drain relays on the IO pool).
        viewModelScope.launch { withContext(Dispatchers.IO) { repository.drainRelayOutbox() } }
    }

    override fun onCleared() {
        teardownScope.launch { meshMutex.withLock { repository.stopMesh() } }
        IrisBleService.stop(appContext)
        super.onCleared()
    }
}