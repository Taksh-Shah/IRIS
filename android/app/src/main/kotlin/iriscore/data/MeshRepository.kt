package iriscore.data

import android.util.Log
import iriscode.FfiInboxListener
import iriscode.FfiIncomingMessage
import iriscode.IrisEngine
import iriscode.IrisFfiException
import iriscore.di.DefaultDispatcher
import iriscore.di.NodeId
import iriscore.identity.KeystoreEd25519
import iriscore.ui.state.InboxUiMessage
import iriscore.ui.state.MeshStatus
import iriscore.ui.state.MeshUiState
import iriscore.util.PeerIdCodec
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Clean-architecture repository bridging the Rust [IrisEngine] to the UI
 * (AC-6 MVVM) and the WorkManager relay path (AC-7). Owns the inbox listener
 * bridge (`FfiInboxListener` foreign trait -> StateFlow).
 */
@Singleton
class MeshRepository @Inject constructor(
    private val engine: IrisEngine,
    @NodeId private val nodeId: ByteArray,
    private val outbox: RelayOutbox,
    private val keystore: KeystoreEd25519,
    @DefaultDispatcher private val dispatcher: CoroutineDispatcher,
) {
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)

    /** Guards [subscribeInbox] against duplicate registration. */
    private val inboxSubscribed = java.util.concurrent.atomic.AtomicBoolean(false)

    private val nodeIdHex = PeerIdCodec.toHex(nodeId)

    private val _uiState = MutableStateFlow(
        MeshUiState.initial(
            nodeIdHex = nodeIdHex,
            identityBackend = keystore.backendType.name,
        ),
    )
    val uiState: StateFlow<MeshUiState> = _uiState.asStateFlow()

    /** Bring the three transports up (BLE + Wi-Fi Aware + Wi-Fi Direct). */
    fun startMesh() {
        try {
            engine.startAll()
            _uiState.update { it.copy(status = MeshStatus.RUNNING) }
        } catch (e: IrisFfiException) {
            // Was a bare `catch (_: IrisFfiException)` — the actual failure
            // reason was discarded, so "the mesh won't start" gave no signal
            // beyond a status flag. There is no Rust-side tracing bridge to
            // logcat either, so this was the only place this information
            // could surface at all.
            Log.w(TAG, "startMesh: engine.startAll() failed", e)
            _uiState.update { it.copy(status = MeshStatus.UNAVAILABLE) }
        }
    }

    fun stopMesh() {
        try {
            engine.stopAll()
        } catch (_: IrisFfiException) {
            // Teardown is best-effort. This used to be try/finally with no
            // catch, so an FFI error propagated into a bare `launch` on a
            // SupervisorJob — which isolates siblings but does NOT handle the
            // exception — and terminated the process during shutdown.
        } finally {
            _uiState.update { it.copy(status = MeshStatus.IDLE) }
        }
    }

    /**
     * Install the inbox listener; engine invokes it from its tokio runtime.
     *
     * Idempotent. This repository is a `@Singleton` but `ensureStarted` is
     * guarded per-ViewModel, so every ViewModel recreation (screen rotation,
     * config change) used to register ANOTHER listener against the same engine:
     * N rotations meant every message rendered N times and N leaked tokio tasks.
     */
    fun subscribeInbox() {
        if (!inboxSubscribed.compareAndSet(false, true)) return
        engine.subscribeInbox(object : FfiInboxListener {
            override fun onMessage(message: FfiIncomingMessage) {
                val ui = message.toUi()
                scope.launch {
                    _uiState.update {
                        // Bounded: the comment claiming the engine bounds this
                        // was wrong — nothing trimmed it on the Kotlin side, so
                        // a long session grew until it OOMed.
                        it.copy(messages = (it.messages + ui).takeLast(MAX_UI_MESSAGES))
                    }
                }
            }
        })
    }

    /**
     * @return `true` if accepted by the engine (or spooled for relay when the
     * mesh is down).
     */
    fun send(recipientHex: String, text: String, priority: UByte): Boolean {
        val normalized = recipientHex.trim().lowercase()
        // A malformed recipient is user input, not a programming error: `check`
        // threw IllegalStateException on the IO dispatcher, which crashed the
        // app on a typo. Report it through the UI state instead.
        if (!PeerIdCodec.isHex(normalized) || normalized.length != RECIPIENT_HEX_LENGTH) {
            _uiState.update { it.copy(lastError = "Recipient must be a $RECIPIENT_HEX_LENGTH-character hex PeerId") }
            return false
        }
        _uiState.update { it.copy(lastError = null) }
        return try {
            engine.sendText(normalized, text, priority)
            // HW-2: this used to be the whole success path — nothing added
            // the sent message to _uiState.messages, so it never appeared in
            // the console at all: no error (this path never throws), no
            // history entry (nothing appends one), indistinguishable from a
            // message that silently vanished regardless of whether the
            // transport actually delivered it. subscribeInbox's listener
            // only fires for RECEIVED messages (FfiInboxListener), so this
            // was the only place a locally-sent message could ever surface.
            _uiState.update {
                it.copy(messages = (it.messages + InboxUiMessage.sent(normalized, text, priority)).takeLast(MAX_UI_MESSAGES))
            }
            true
        } catch (_: IrisFfiException) {
            scope.launch {
                outbox.enqueue(RelayOutbox.QueuedMessage(normalized, text, priority))
                _uiState.update {
                    it.copy(
                        relayQueued = outbox.size,
                        messages = (it.messages + InboxUiMessage.pending(normalized, text, priority)).takeLast(MAX_UI_MESSAGES),
                    )
                }
            }
            false
        }
    }

    /** Spill the relay outbox into the engine (WorkManager entrypoint). */
    suspend fun drainRelayOutbox(): Int {
        val sent = outbox.drain { queued ->
            try {
                engine.sendText(queued.recipientHex, queued.text, queued.priority)
                true
            } catch (_: IrisFfiException) {
                false
            }
        }
        if (sent > 0) {
            _uiState.update { it.copy(relayQueued = outbox.size) }
        }
        return sent
    }

    val pendingRelayCount: Int get() = outbox.size

    companion object {
        private const val TAG = "IrisMeshRepository"

        /** A PeerId is a 32-byte key rendered as hex (PeerIdCodec.toHex). */
        const val RECIPIENT_HEX_LENGTH = 64

        /** Ceiling on retained UI messages; the engine holds the durable copy. */
        const val MAX_UI_MESSAGES = 500

        private fun FfiIncomingMessage.toUi(): InboxUiMessage = InboxUiMessage.received(
            senderId = PeerIdCodec.toHex(senderId),
            payloadUtf8 = payload.toString(Charsets.UTF_8),
            priority = priority,
            receivedAtMs = receivedAtMs.toLong(),
        )
    }
}