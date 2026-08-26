package iriscore.ui

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import iriscore.command.CommandExecutor
import iriscore.command.CommandResult
import iriscore.data.MeshRepository
import iriscore.service.IrisBleService
import iriscore.ui.state.ConsoleEntry
import iriscore.ui.state.MeshStatus
import iriscore.ui.state.MeshUiState
import iriscore.util.MeshPermissions
import iriscore.worker.WorkScheduler
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.CoroutineExceptionHandler
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
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

    /** System output and command echoes, newest last. */
    private val _consoleEvents = MutableStateFlow<List<ConsoleEntry>>(emptyList())

    /** Active `/search` filter; null when unfiltered. */
    private val _searchQuery = MutableStateFlow<String?>(null)
    val searchQuery: StateFlow<String?> = _searchQuery.asStateFlow()

    /** Recipient set via `/to` or `@`. */
    private val _recipient = MutableStateFlow<String?>(null)
    val recipient: StateFlow<String?> = _recipient.asStateFlow()

    /** Messages the user cleared with `/clear` stay hidden without being deleted. */
    private val _clearedBeforeMs = MutableStateFlow(0L)

    /**
     * The console stream: mesh messages and system output merged in time order,
     * with the `/search` filter applied. Derived rather than stored so the
     * repository stays the single source of truth for messages.
     */
    val console: StateFlow<List<ConsoleEntry>> =
        combine(
            repository.uiState,
            _consoleEvents,
            _searchQuery,
            _clearedBeforeMs,
        ) { state, events, query, clearedBefore ->
            val messages = state.messages
                .filter { it.receivedAtMs >= clearedBefore }
                .filter { query == null || it.payloadUtf8.contains(query, ignoreCase = true) }
                .map { ConsoleEntry.Message(it) }

            val system = events.filter { it.atMs >= clearedBefore }

            (messages + system).sortedBy { it.atMs }
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), emptyList())

    // AND-RT-101: engine bring-up/teardown are blocking FFI calls — never on
    // the main thread. Serialized so start/stop cannot interleave.
    private val meshMutex = Mutex()

    // viewModelScope is cancelled before onCleared() runs, so teardown uses a
    // scope that survives until the engine actually stops.
    // A SupervisorJob isolates siblings but does NOT handle exceptions; without
    // a handler an escaping throw reaches the default handler and terminates
    // the process during teardown.
    private val teardownScope = CoroutineScope(
        SupervisorJob() +
            Dispatchers.Default +
            CoroutineExceptionHandler { _, _ -> /* teardown is best-effort */ },
    )

    /** Guards [ensureStarted] so repeated recompositions bring the mesh up once. */
    private val started = AtomicBoolean(false)

    init {
        // Bring the mesh up immediately only when the runtime grants are already
        // in place (returning user). Otherwise the shell requests them first and
        // calls [ensureStarted]; starting the radios without BLUETOOTH_SCAN /
        // NEARBY_WIFI_DEVICES yields a mesh that reports RUNNING while silently
        // carrying no traffic.
        if (MeshPermissions.allGranted(appContext)) ensureStarted()
    }

    /**
     * Idempotent transport bring-up. Safe to call from a composable effect on
     * every recomposition and after a permission result.
     */
    fun ensureStarted() {
        if (!started.compareAndSet(false, true)) return
        // Boot is best-effort: an unavailable radio must not crash the shell.
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
            // ForegroundServiceStartNotAllowedException / radio unavailable.
            // Reset so a later attempt (permission granted, radio switched on)
            // can retry instead of latching into a half-started state.
            started.set(false)
        }
    }

    fun send(recipientHex: String, text: String, priority: UByte = 0u) {
        if (recipientHex.isBlank() || text.isBlank()) return
        // engine.sendText is a blocking FFI call — never on the main thread.
        viewModelScope.launch { withContext(Dispatchers.IO) { repository.send(recipientHex, text, priority) } }
    }

    /**
     * Runs one console line — a message, a `/command`, or an `@reference`.
     *
     * Parsing and dispatch live in [CommandExecutor] so the behaviour of any
     * input is testable without a Compose or Android runtime; this method only
     * applies the resulting effect.
     */
    fun submit(raw: String) {
        val line = raw.trim()
        if (line.isEmpty()) return

        // Echo commands and references so the transcript reads back; a plain
        // message needs no echo, it appears as a message.
        if (line.startsWith('/') || line.startsWith('@')) {
            appendEvent(ConsoleEntry.echo(line))
        }

        when (val result = CommandExecutor.execute(line, hasRecipient = _recipient.value != null)) {
            is CommandResult.Send -> send(_recipient.value.orEmpty(), result.text, result.priority)

            is CommandResult.SetRecipient -> {
                _recipient.value = result.peerIdHex
                appendEvent(
                    ConsoleEntry.system(
                        title = "RECIPIENT",
                        lines = listOf("peer" to result.peerIdHex.take(16) + "…"),
                        status = "READY",
                    ),
                )
            }

            is CommandResult.Search -> {
                _searchQuery.value = result.query
                appendEvent(
                    ConsoleEntry.system(
                        title = "SEARCH",
                        lines = listOf("query" to (result.query ?: "(cleared)")),
                    ),
                )
            }

            CommandResult.Relay -> {
                syncRelayNow()
                appendEvent(
                    ConsoleEntry.system(
                        title = "RELAY.DRAIN",
                        lines = listOf("queued" to uiState.value.relayQueued.toString()),
                        status = "DISPATCHED",
                    ),
                )
            }

            CommandResult.Clear -> {
                // Hide rather than delete: the engine still owns the messages,
                // and a cleared view must not imply data loss.
                _clearedBeforeMs.value = System.currentTimeMillis()
                _consoleEvents.value = emptyList()
            }

            is CommandResult.System -> appendEvent(resolveSystem(result))

            is CommandResult.Error -> appendEvent(
                ConsoleEntry.system(title = "ERROR", lines = listOf("detail" to result.message)),
            )

            CommandResult.None -> Unit
        }
    }

    /**
     * Fills in the system blocks that need live state. The executor names what
     * was asked for; only the view model can answer it.
     */
    private fun resolveSystem(result: CommandResult.System): ConsoleEntry.System {
        val state = uiState.value
        return when (result.title) {
            "NODE" -> ConsoleEntry.system(
                title = "NODE",
                lines = listOf(
                    // Full hex, not nodeIdShort: this is the one place a user
                    // can read their own PeerId to hand to a peer for `/to` or
                    // `@` addressing, and both require the full 64-char id.
                    "id" to state.nodeIdHex,
                    "identity" to state.identityBackend,
                ),
                status = state.status.name,
            )

            "LINKS" -> ConsoleEntry.system(
                title = "LINKS",
                lines = listOf(
                    "transport" to state.status.name,
                    "relay" to state.relayQueued.toString(),
                    "messages" to state.messages.size.toString(),
                ),
                status = if (state.status == MeshStatus.RUNNING) "UP" else "DOWN",
            )

            else -> ConsoleEntry.system(result.title, result.lines)
        }
    }

    private fun appendEvent(entry: ConsoleEntry) {
        // Bounded: a long session must not grow the console without limit.
        _consoleEvents.value = (_consoleEvents.value + entry).takeLast(MAX_CONSOLE_EVENTS)
    }

    fun syncRelayNow() {
        // engine.sendText is a blocking FFI call — never on the main thread
        // (AND-RT-101/AND-RT-101-R3 re-review: drain relays on the IO pool).
        viewModelScope.launch { withContext(Dispatchers.IO) { repository.drainRelayOutbox() } }
    }

    companion object {
        /** Console event cap; messages themselves are bounded by the engine. */
        private const val MAX_CONSOLE_EVENTS = 200
    }

    /**
     * Deliberately does NOT stop the mesh.
     *
     * `onCleared` runs on every ViewModel destruction, including a screen
     * rotation. It used to call `repository.stopMesh()`, which reaches
     * `MessageEngine::shutdown()` — a terminal operation — on an engine that is
     * a `@Singleton`. One rotation therefore killed the mesh permanently for
     * the rest of the process, while the UI happily reported RUNNING again
     * because `startMesh()` set the status without the engine being alive.
     *
     * Mesh lifetime belongs to the foreground service, which is the component
     * whose whole purpose is outliving the UI. The service is stopped by
     * [stopMesh], invoked when the user actually leaves the mesh — not when the
     * screen turns.
     */
    override fun onCleared() {
        super.onCleared()
    }

    /** Explicit user-initiated teardown: stop the transports and the service. */
    fun stopMesh() {
        teardownScope.launch { meshMutex.withLock { repository.stopMesh() } }
        IrisBleService.stop(appContext)
        started.set(false)
    }
}