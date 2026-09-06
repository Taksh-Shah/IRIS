package iriscore.ui

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import iriscore.command.CommandExecutor
import iriscore.command.CommandResult
import iriscore.data.AppPreferences
import iriscore.data.ContactStore
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
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import iriscore.ui.state.TransportStatus
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
    private val contactStore: ContactStore,
    private val appPreferences: AppPreferences,
) : ViewModel() {

    val uiState: StateFlow<MeshUiState> = repository.uiState

    /** System output and command echoes, newest last. */
    private val _consoleEvents = MutableStateFlow<List<ConsoleEntry>>(emptyList())

    /**
     * HV-59: per-transport connection state for the status line. Polled every
     * 5 s from [MeshRepository.transportStatuses] while the mesh is RUNNING.
     * Empty before first poll and while IDLE/UNAVAILABLE.
     */
    private val _transportStates = MutableStateFlow<List<TransportStatus>>(emptyList())
    val transportStates: StateFlow<List<TransportStatus>> = _transportStates.asStateFlow()

    /**
     * HV-56: contact book — peerIdHex → user-assigned name. Loaded from
     * [ContactStore] on init and updated on every `/name` write so the UI
     * reflects the new name without a restart.
     */
    private val _contacts = MutableStateFlow<Map<String, String>>(emptyMap())
    val contacts: StateFlow<Map<String, String>> = _contacts.asStateFlow()

    /**
     * HV-60: tracks how many reconnect attempts have been made since the mesh
     * last came up. Resets to 0 when the engine returns to RUNNING so normal
     * operation clears the escalated help text in RetryNotice.
     */
    private val _reconnectAttempts = MutableStateFlow(0)
    val reconnectAttempts: StateFlow<Int> = _reconnectAttempts.asStateFlow()

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
        _contacts.value = contactStore.all()
        // HV-60: reset the retry counter whenever the engine comes back up so
        // the escalated "restart the app" guidance clears on a successful reconnect.
        viewModelScope.launch {
            uiState.collect { s -> if (s.status == MeshStatus.RUNNING) _reconnectAttempts.value = 0 }
        }
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
                // HV-62: emit the WELCOME block once per install so the user
                // sees their node id and tips without having to type /node.
                if (!appPreferences.welcomeShown) {
                    appPreferences.welcomeShown = true
                    val nodeId = uiState.value.nodeIdHex
                    appendEvent(
                        ConsoleEntry.system(
                            title = "WELCOME",
                            lines = listOf(
                                "node" to nodeId,
                                "tip" to "share your id with peers so they can /to you",
                                "tip" to "/help  lists all commands",
                                "tip" to "/name <peer-id> <name>  saves a contact",
                            ),
                            status = "FIRST RUN",
                        ),
                    )
                }
            }
            startTransportPoll()
        } catch (_: RuntimeException) {
            // ForegroundServiceStartNotAllowedException / radio unavailable.
            // Reset so a later attempt (permission granted, radio switched on)
            // can retry instead of latching into a half-started state.
            started.set(false)
        }
    }

    /**
     * HV-59: poll the engine's per-transport state every 5 s while the mesh is
     * RUNNING. The snapshot call is blocking FFI — kept on IO. On any failure
     * we clear the display rather than showing stale data.
     */
    private fun startTransportPoll() {
        viewModelScope.launch(Dispatchers.IO) {
            while (true) {
                delay(5_000L)
                if (uiState.value.status == MeshStatus.RUNNING) {
                    _transportStates.value = try {
                        repository.transportStatuses()
                    } catch (_: Exception) {
                        emptyList()
                    }
                } else {
                    _transportStates.value = emptyList()
                }
            }
        }
    }

    fun send(recipientHex: String, text: String, priority: UByte = 0u) {
        if (recipientHex.isBlank() || text.isBlank()) return
        // engine.sendText is a blocking FFI call — never on the main thread.
        viewModelScope.launch { withContext(Dispatchers.IO) { repository.send(recipientHex, text, priority) } }
    }

    /** HV-41: send to every peer in range — see [CommandResult.Broadcast]. */
    fun broadcast(text: String, priority: UByte) {
        if (text.isBlank()) return
        viewModelScope.launch { withContext(Dispatchers.IO) { repository.broadcast(text, priority) } }
    }

    /**
     * HV-57: set the recipient from a tapped received message, the same
     * effect as running `/to <peerIdHex>` by hand. Before this, replying
     * required copying the sender's full PeerId (not even fully shown) and
     * typing `/to` yourself — in practice conversation was one-directional
     * unless both sides pre-exchanged hex ids.
     */
    fun replyTo(peerIdHex: String) {
        _recipient.value = peerIdHex
        appendEvent(
            ConsoleEntry.system(
                title = "RECIPIENT",
                lines = listOf("peer" to peerIdHex.take(16) + "…"),
                status = "REPLYING",
            ),
        )
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

        // HV-56: provide a contact resolver so /to and @ accept saved names.
        val contactResolver: (String) -> String? = { name ->
            _contacts.value.entries.firstOrNull { it.value.lowercase() == name.lowercase() }?.key
        }
        when (val result = CommandExecutor.execute(line, hasRecipient = _recipient.value != null, contactResolver = contactResolver)) {
            is CommandResult.Send -> send(_recipient.value.orEmpty(), result.text, result.priority)

            is CommandResult.Broadcast -> broadcast(result.text, result.priority)

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

            is CommandResult.AddKey -> {
                viewModelScope.launch {
                    val line = withContext(Dispatchers.IO) {
                        try {
                            repository.addPeerKey(result.peerIdHex, result.x25519Hex)
                            "peer ${result.peerIdHex.take(16)}…" to "key trusted"
                        } catch (e: RuntimeException) {
                            "error" to (e.message ?: e.javaClass.simpleName)
                        }
                    }
                    appendEvent(ConsoleEntry.system(title = "ADDKEY", lines = listOf(line)))
                }
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

            is CommandResult.System ->
                if (result.title == "DIAG" || result.title == "STATS" || result.title == "X25519" || result.title == "LINKS") {
                    // These read the engine over a blocking FFI call — off-main.
                    viewModelScope.launch {
                        val entry = withContext(Dispatchers.IO) {
                            when (result.title) {
                                "STATS" -> resolveStats()
                                "X25519" -> resolveX25519()
                                "LINKS" -> resolveLinks()
                                else -> resolveDiag()
                            }
                        }
                        appendEvent(entry)
                    }
                } else {
                    appendEvent(resolveSystem(result))
                }

            is CommandResult.SaveContact -> {
                viewModelScope.launch(Dispatchers.IO) {
                    contactStore.save(result.peerIdHex, result.name)
                    _contacts.value = contactStore.all()
                    appendEvent(
                        ConsoleEntry.system(
                            title = "CONTACT",
                            lines = listOf(
                                "peer" to result.peerIdHex.take(16) + "…",
                                "name" to result.name,
                            ),
                            status = "SAVED",
                        ),
                    )
                }
            }

            CommandResult.ListContacts -> {
                val all = _contacts.value
                val lines = if (all.isEmpty()) {
                    listOf("(none)" to "use /name <peer-id> <name> to add one")
                } else {
                    all.entries.sortedBy { it.value.lowercase() }
                        .map { it.value to it.key.take(16) + "…" }
                }
                appendEvent(ConsoleEntry.system(title = "CONTACTS", lines = lines))
            }

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

            else -> ConsoleEntry.system(result.title, result.lines)
        }
    }

    /**
     * HV-40: renders `/peers` — the mesh topology `/diag` already computed but
     * `/peers` never showed. Before this, `/peers` returned only a status-name
     * echo and message/relay counts — none of it the neighbour graph a user
     * actually needs to debug "the mesh never widens past one peer." The
     * `NeighborTable` data (`snap.neighbors`) already existed in
     * `FfiMeshSnapshot` and was already rendered by `/diag`
     * ([resolveDiag]) — this reuses the same field, just as its own
     * focused, undiluted view instead of one line buried in the full
     * diagnostic dump.
     *
     * Does not yet show peers that were discovered but never linked:
     * `NeighborTable`'s `NeighborState` has only `LinkedUp`/`LinkedDown` — a
     * scan result that never became a connection isn't tracked anywhere
     * accessible today. Left as a follow-up (would need new engine-level
     * state, not just a UI change) rather than folded into this fix.
     */
    private fun resolveLinks(): ConsoleEntry {
        val snap = try {
            repository.snapshot()
        } catch (e: RuntimeException) {
            return ConsoleEntry.system("LINKS", listOf("error" to (e.message ?: e.javaClass.simpleName)))
        }
        val lines = buildList {
            if (snap.neighbors.isEmpty()) {
                add("neighbors" to "none")
            } else {
                snap.neighbors.forEach { n ->
                    add(
                        (contactNameFor(n.peerIdHex) ?: "peer ${n.peerIdHex.take(16)}…") to
                            "${n.state}  links=[${n.links.joinToString(", ")}]",
                    )
                }
            }
            // HV-38: `relayQueued` (the "Q3" status chip) is this node's own
            // unsent-message backlog (`RelayOutbox`/`WorkManager`), not the
            // core's real mesh-relay activity (forwarding *other* peers'
            // messages, `enqueue_relay`/`metrics.relayed`) — the two were easy
            // to conflate since both are named "relay". Show them side by
            // side, distinctly labeled, rather than only the local one.
            add("pending sends (local)" to uiState.value.relayQueued.toString())
            add("relayed (mesh, lifetime)" to snap.messages.relayed.toString())
        }
        return ConsoleEntry.system(
            title = "LINKS",
            lines = lines,
            status = if (uiState.value.status == MeshStatus.RUNNING) "UP" else "DOWN",
        )
    }

    /** HV-40/HV-56: resolve a peer to its saved contact name, if any. */
    private fun contactNameFor(peerIdHex: String): String? = _contacts.value[peerIdHex]

    /**
     * HV-3: renders `/diag` from the engine's live [MeshRepository.snapshot].
     * Runs on IO (the underlying FFI call blocks). A failure is surfaced as a
     * system line rather than crashing the console.
     */
    private fun resolveDiag(): ConsoleEntry {
        val snap = try {
            repository.snapshot()
        } catch (e: RuntimeException) {
            return ConsoleEntry.system("DIAG", listOf("error" to (e.message ?: e.javaClass.simpleName)))
        }
        val lines = buildList {
            add("node" to snap.nodeIdHex)
            snap.transports.forEach { t ->
                add(
                    t.id to "${t.state}  batt=${t.estimatedBatteryMa}mA  bw=${t.bandwidthAvailableBps}bps  cong=${t.congestionLevel}",
                )
            }
            if (snap.neighbors.isEmpty()) {
                add("neighbors" to "none")
            } else {
                snap.neighbors.forEach { n ->
                    add(
                        "peer ${n.peerIdHex.take(16)}…" to
                            "${n.state}  links=[${n.links.joinToString(", ")}]",
                    )
                }
            }
            snap.messages.let { m ->
                add(
                    "messages" to
                        "sent=${m.sent} delivered=${m.delivered} relayed=${m.relayed} " +
                        "dup=${m.droppedDuplicates} expired=${m.expired} failed=${m.deliveryFailed}",
                )
            }
            // HV-86: the tail of the transport-event ring — newest last, so the
            // run-up to a failure reads top-to-bottom.
            val events = snap.recentEvents
            if (events.isEmpty()) {
                add("events" to "none yet")
            } else {
                events.takeLast(DIAG_EVENT_LINES).forEach { e ->
                    add("${e.level.take(1)} ${e.event}" to e.detail)
                }
            }
        }
        return ConsoleEntry.system(title = "DIAG", lines = lines, status = uiState.value.status.name)
    }

    /**
     * HV-6: renders `/stats` — the non-zero raw `MetricsRegistry` counters from
     * the engine. `snapshot()` also emits an `iris.kpi` logcat line, so a bench
     * `adb logcat | grep iris.kpi` capture is a counter time series.
     */
    /** HV-89 interim: show this node's X25519 static key for a peer to `/addkey`. */
    private fun resolveX25519(): ConsoleEntry {
        val line = try {
            "x25519" to repository.staticX25519()
        } catch (e: RuntimeException) {
            "error" to (e.message ?: e.javaClass.simpleName)
        }
        return ConsoleEntry.system(title = "X25519", lines = listOf(line))
    }

    private fun resolveStats(): ConsoleEntry {
        val snap = try {
            repository.snapshot()
        } catch (e: RuntimeException) {
            return ConsoleEntry.system("STATS", listOf("error" to (e.message ?: e.javaClass.simpleName)))
        }
        val nonZero = snap.counters.filter { it.value > 0uL }
        val lines = if (nonZero.isEmpty()) {
            listOf("counters" to "all zero")
        } else {
            nonZero.map { it.name to it.value.toString() }
        }
        return ConsoleEntry.system(title = "STATS", lines = lines, status = uiState.value.status.name)
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

        /** HV-86: how many ring events `/diag` prints (the ring holds ~128). */
        private const val DIAG_EVENT_LINES = 15
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

    /**
     * AN-10: reconnect after an UNAVAILABLE failure.
     *
     * [stopMesh] resets [started] so [ensureStarted] can re-enter; both paths go
     * through [meshMutex] so teardown completes before the restart acquires the lock.
     */
    fun reconnectMesh() {
        _reconnectAttempts.value++
        stopMesh()
        ensureStarted()
    }
}