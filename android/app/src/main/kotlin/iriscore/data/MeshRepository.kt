package iriscore.data

import android.util.Log
import dagger.Lazy
import iriscode.FfiInboxListener
import iriscode.FfiIncomingMessage
import iriscode.IrisEngine
import iriscode.IrisFfiException
import iriscore.adapter.AndroidNsdAdapter
import iriscore.adapter.AndroidInternetNetworkMonitor
import iriscore.di.DefaultDispatcher
import iriscore.di.NodeId
import iriscore.identity.KeystoreEd25519
import iriscore.ui.state.InboxUiMessage
import iriscore.ui.state.MeshStatus
import iriscore.ui.state.MeshUiState
import iriscore.util.AdvertisementCodec
import iriscore.util.PeerIdCodec
import iriscore.util.PairingCodeCodec
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Clean-architecture repository bridging the Rust [IrisEngine] to the UI
 * (AC-6 MVVM) and the WorkManager relay path (AC-7). Owns the inbox listener
 * bridge (`FfiInboxListener` foreign trait -> StateFlow).
 */
@Singleton
class MeshRepository @Inject constructor(
    private val engine: Lazy<IrisEngine>,
    @NodeId private val nodeId: ByteArray,
    private val outbox: RelayOutbox,
    private val keystore: KeystoreEd25519,
    private val knownPeers: KnownPeersStore,
    private val trustedPeers: TrustedPeerStore,
    private val nsdAdapter: AndroidNsdAdapter,
    private val internetMonitor: AndroidInternetNetworkMonitor,
    @DefaultDispatcher private val dispatcher: CoroutineDispatcher,
) {
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)

    /** Guards [subscribeInbox] against duplicate registration. */
    private val inboxSubscribed = AtomicBoolean(false)
    private val meshRequested = AtomicBoolean(false)
    private val activationJobs = ConcurrentHashMap<String, Job>()
    private val activationMutex = Mutex()
    private val lifecycleLock = Any()

    private val nodeIdHex = PeerIdCodec.toHex(nodeId)

    private val _uiState = MutableStateFlow(
        MeshUiState.initial(
            nodeIdHex = nodeIdHex,
            identityBackend = keystore.backendType.name,
        ),
    )
    val uiState: StateFlow<MeshUiState> = _uiState.asStateFlow()

    init {
        internetMonitor.onNetworkStateChanged = { state ->
            scope.launch {
                synchronized(lifecycleLock) {
                    if (!meshRequested.get() || internetMonitor.currentState() != state) {
                        return@synchronized
                    }
                    engine.get().setInternetNetworkState(state.validatedWan, state.localNetwork)
                    if (state.localNetwork) nsdAdapter.start() else nsdAdapter.stop()
                    if (state.validatedWan) {
                        activateVerifiedInternetPeers()
                    } else {
                        cancelInternetActivations()
                    }
                    reconcileMeshState()
                }
            }
        }
    }

    private fun activateVerifiedInternetPeers() {
        trustedPeers.all()
            .filterValues { it.verified }
            .keys
            .forEach(::scheduleInternetActivation)
    }

    /** Retry trusted relay sessions while validated WAN remains available. */
    private fun scheduleInternetActivation(peerHex: String) {
        if (!meshRequested.get() || !internetMonitor.isValidatedWanAvailable()) return
        synchronized(activationJobs) {
            if (activationJobs[peerHex]?.isActive == true) return
            val job = scope.launch {
                var retryDelayMs = INTERNET_RETRY_INITIAL_MS
                try {
                    while (meshRequested.get() && internetMonitor.isValidatedWanAvailable()) {
                        val activated = activationMutex.withLock {
                            runCatching {
                                engine.get().activateInternetPeer(peerHex)
                            }.onFailure {
                                Log.i(TAG, "Internet peer $peerHex not active yet: ${it.message}")
                            }.isSuccess
                        }
                        if (activated) return@launch
                        delay(retryDelayMs)
                        retryDelayMs = (retryDelayMs * 2).coerceAtMost(INTERNET_RETRY_MAX_MS)
                    }
                } finally {
                    currentCoroutineContext()[Job]?.let { activationJobs.remove(peerHex, it) }
                }
            }
            activationJobs[peerHex] = job
        }
    }

    private fun cancelInternetActivations() {
        activationJobs.values.forEach(Job::cancel)
        activationJobs.clear()
    }

    /** Re-evaluate startAll after late Android network callbacks or recovery. */
    private fun reconcileMeshState() {
        if (!meshRequested.get()) return
        runCatching { engine.get().startAll() }
            .onSuccess { _uiState.update { it.copy(status = MeshStatus.RUNNING) } }
            .onFailure { error ->
                Log.i(TAG, "Mesh is waiting for a usable transport: ${error.message}")
                _uiState.update { it.copy(status = MeshStatus.UNAVAILABLE) }
            }
    }

    /** Bring all transports up, including Internet relay and authenticated LAN. */
    fun startMesh() {
        synchronized(lifecycleLock) {
            meshRequested.set(true)
            try {
            // HV-21 (legacy path): re-feed the old bare-key store too. Nothing
            // currently writes to it (see `addPeerKey`'s doc), and it is no
            // longer the engine's active key directory (see
            // `IrisEngine::build`'s `set_key_directory`) — kept only so any
            // pre-existing entries from before this change are not silently
            // lost. Safe to remove once confirmed no device has old entries.
            for ((peerHex, x25519Hex) in knownPeers.all()) {
                runCatching { engine.get().registerPeerKey(peerHex, x25519Hex) }
            }
            // Trusted-peers: the engine's TrustStore is in-memory only and
            // rebuilt empty on every process start — re-adopt every persisted
            // advertisement before starting transports, so a peer trusted
            // before the restart is still encryptable-to (and still shows its
            // prior trust level) on the first send after a cold start. This
            // mirrors the HV-21 pattern above for the new trust-store-backed
            // path.
            for ((peerHex, record) in trustedPeers.all()) {
                val identity = PeerIdCodec.fromHex(peerHex)
                val x25519 = PeerIdCodec.fromHex(record.x25519Hex)
                val sig = PeerIdCodec.fromHex(record.sigHex)
                if (identity == null || x25519 == null || sig == null) continue
                val ad = iriscode.FfiPeerAdvertisement(
                    identityPubkey = identity,
                    x25519Pubkey = x25519,
                    keyGenCounter = record.keyGenCounter.toULong(),
                    validUntil = record.validUntil.toULong(),
                    sig = sig,
                )
                runCatching {
                    engine.get().adoptPeerAdvertisement(ad)
                    if (record.verified) {
                        engine.get().verifyPeer(identity, x25519)
                    }
                }
            }
            // Connectivity callbacks are asynchronous. Keep the requested
            // state alive after an initial Unavailable result so a late WAN
            // validation or LAN appearance can reconcile without another tap.
            internetMonitor.start()
            val network = internetMonitor.currentState()
            engine.get().setInternetNetworkState(network.validatedWan, network.localNetwork)
            reconcileMeshState()
            if (network.localNetwork) nsdAdapter.start()
            activateVerifiedInternetPeers()
            } catch (e: Exception) {
            // AN-9: broadened from IrisFfiException — any unexpected exception
            // (ClassCastException, NullPointerException, etc.) would otherwise
            // escape into the SupervisorJob coroutine and crash the process.
            Log.w(TAG, "startMesh: engine.startAll() failed", e)
            _uiState.update { it.copy(status = MeshStatus.UNAVAILABLE) }
            }
        }
    }

    fun stopMesh() {
        synchronized(lifecycleLock) {
            meshRequested.set(false)
            cancelInternetActivations()
            internetMonitor.stop()
            engine.get().setInternetNetworkState(false, false)
            nsdAdapter.stop()
            try {
                engine.get().stopAll()
            } catch (_: IrisFfiException) {
            // Teardown is best-effort. This used to be try/finally with no
            // catch, so an FFI error propagated into a bare `launch` on a
            // SupervisorJob — which isolates siblings but does NOT handle the
            // exception — and terminated the process during shutdown.
            } finally {
                _uiState.update { it.copy(status = MeshStatus.IDLE) }
            }
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
        engine.get().subscribeInbox(object : FfiInboxListener {
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
            engine.get().sendText(normalized, text, priority)
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

    /**
     * HV-41: send to every peer in range. Unlike [send], there is no
     * recipient to validate — the core (`is_broadcast`/`deliver_or_relay`)
     * treats an empty `recipient_id` as "deliver locally and relay to
     * everyone." No relay-outbox spooling on failure: a broadcast that the
     * transport layer cannot send right now has no single peer to retry
     * against later, unlike an addressed message.
     *
     * @return `true` if accepted by the engine.
     */
    fun broadcast(text: String, priority: UByte): Boolean {
        _uiState.update { it.copy(lastError = null) }
        return try {
            engine.get().broadcastText(text, priority)
            _uiState.update {
                it.copy(messages = (it.messages + InboxUiMessage.sent(BROADCAST_LABEL, text, priority)).takeLast(MAX_UI_MESSAGES))
            }
            true
        } catch (e: IrisFfiException) {
            _uiState.update { it.copy(lastError = "Broadcast failed: ${e.message}") }
            false
        }
    }

    /**
     * HV-3: point-in-time mesh diagnostic (`/diag`). Blocking FFI call — the
     * caller must be off the main thread. Also emits an `iris.diag` line to
     * logcat.
     */
    fun snapshot(): iriscode.FfiMeshSnapshot = engine.get().snapshot()

    /**
     * HV-59: per-transport connection state for the status line. Reuses the
     * same [snapshot] call so the engine's event ring is also refreshed; the
     * ViewModel polls this every 5 s while RUNNING.
     */
    fun transportStatuses(): List<iriscore.ui.state.TransportStatus> =
        engine.get().snapshot().transports.map { t ->
            iriscore.ui.state.TransportStatus(
                label = when {
                    t.id.startsWith("ble") -> "BLE"
                    t.id.startsWith("wifi-direct") || t.id.startsWith("wifidirect") || t.id.startsWith("wd") -> "WD"
                    t.id.startsWith("internet") || t.id.startsWith("net") || t.id.startsWith("tcp") -> "NET"
                    t.id.startsWith("wifi-aware") || t.id.startsWith("wifiaware") || t.id.startsWith("nan") -> "NAN"
                    else -> t.id.take(3).uppercase()
                },
                connected = t.state == "Connected",
                state = t.state,
            )
        }

    /** HV-89 interim: this node's X25519 static public key, 64-hex. */
    fun staticX25519(): String =
        engine.get().staticX25519Pubkey().joinToString("") { "%02x".format(it) }

    /**
     * HV-89 interim: trust a peer's X25519 key so addressed mail to it can be
     * sealed. HV-21: also persisted so the trust survives an app restart
     * ([startMesh] re-feeds it).
     *
     * Superseded by [addTrustedPeer] — `/addkey` no longer calls this, and
     * nothing else does either (kept only in case a device has old
     * [KnownPeersStore] entries from before that change; see [startMesh]).
     */
    fun addPeerKey(peerHex: String, x25519Hex: String) {
        engine.get().registerPeerKey(peerHex, x25519Hex)
        runCatching { knownPeers.put(peerHex, x25519Hex) }
    }

    /**
     * Trusted-peers: this node's own signed pairing code — share it (paste,
     * or later a QR code) for a peer to `/addkey`. Signing happens via the
     * existing Keystore-backed [keystore] (the private key never leaves the
     * TEE); the bytes it signs are computed on the Rust side so the
     * signature verifies against exactly what [addTrustedPeer] reconstructs.
     */
    fun myAdvertisement(): String {
        val x25519 = engine.get().staticX25519Pubkey()
        val signable = engine.get().advertisementSignableBytes(x25519)
        val sig = keystore.sign(signable)
        val ad = iriscode.FfiPeerAdvertisement(
            identityPubkey = nodeId,
            x25519Pubkey = x25519,
            keyGenCounter = 0uL,
            validUntil = 0uL,
            sig = sig,
        )
        return AdvertisementCodec.encode(ad)
    }

    /** Outcome of [addTrustedPeer]. */
    sealed interface AddPeerOutcome {
        data class Adopted(val peerIdHex: String, val outcome: iriscode.FfiAdoptionOutcome) : AddPeerOutcome
        /** `blob` was not a validly-shaped pairing code (wrong length/not hex). */
        data object InvalidBlob : AddPeerOutcome
        data object SelfPairing : AddPeerOutcome
        data object PersistenceFailed : AddPeerOutcome
    }

    /**
     * Trusted-peers: decode and adopt a peer's `/myadvert` pairing code into
     * the engine's trust store. Persists the advertisement (unverified)
     * so [startMesh] can re-feed it after a restart; a `KeyChangeWarn`
     * outcome means this identity previously presented a different key and
     * sending stays blocked until `/fingerprint <name> confirm` re-verifies.
     */
    fun addTrustedPeer(blob: String): AddPeerOutcome {
        val canonical = PairingCodeCodec.toAdvertisementHex(blob) ?: return AddPeerOutcome.InvalidBlob
        val ad = AdvertisementCodec.decode(canonical) ?: return AddPeerOutcome.InvalidBlob
        if (ad.identityPubkey.contentEquals(nodeId)) return AddPeerOutcome.SelfPairing
        val existing = trustedPeers.get(PeerIdCodec.toHex(ad.identityPubkey))
        // AdvertisementCodec.decode already guarantees each field's exact
        // byte length, so adoptPeerAdvertisement should never throw here —
        // but AN-9 (this file) established that an FFI call left unguarded
        // in a viewModelScope coroutine crashes the process on the first
        // unexpected exception, so this stays defensive rather than assume.
        val outcome = try {
            engine.get().adoptPeerAdvertisement(ad)
        } catch (_: IrisFfiException) {
            return AddPeerOutcome.InvalidBlob
        }
        val peerIdHex = PeerIdCodec.toHex(ad.identityPubkey)
        // Only persist this advertisement as the peer's canonical key when
        // the engine actually adopted it as such. On KEY_CHANGE_WARN /
        // REVOKED / REJECTED the live TrustStore keeps the OLD key on file
        // (adopt_advertisement's KeyChangeWarn branch never overwrites
        // static_x25519_pubkey) or refuses the advertisement outright —
        // persisting the new key here regardless would let `startMesh`'s
        // cold-start replay re-feed the conflicting/rejected key as if it
        // were a fresh, legitimate first-time trust, erasing the conflict
        // instead of preserving it.
        val adoptedAsCanonical = when (outcome) {
            iriscode.FfiAdoptionOutcome.BOUND_UNVERIFIED,
            iriscode.FfiAdoptionOutcome.REFRESHED,
            iriscode.FfiAdoptionOutcome.ROTATION_ADOPTED -> true
            // Core uses DUPLICATE for both an identical repeat and a stale
            // lower-counter replay. Never let the submitted tuple overwrite
            // a different durable winner.
            iriscode.FfiAdoptionOutcome.DUPLICATE -> existing == null ||
                (existing.x25519Hex.equals(PeerIdCodec.toHex(ad.x25519Pubkey), true) &&
                    existing.keyGenCounter == ad.keyGenCounter.toLong() &&
                    existing.sigHex.equals(PeerIdCodec.toHex(ad.sig), true))
            iriscode.FfiAdoptionOutcome.KEY_CHANGE_WARN,
            iriscode.FfiAdoptionOutcome.REVOKED,
            iriscode.FfiAdoptionOutcome.REJECTED,
            -> false
        }
        if (adoptedAsCanonical) {
            val persisted = runCatching {
                trustedPeers.put(
                    peerIdHex = peerIdHex,
                    x25519Hex = PeerIdCodec.toHex(ad.x25519Pubkey),
                    keyGenCounter = ad.keyGenCounter.toLong(),
                    validUntil = ad.validUntil.toLong(),
                    sigHex = PeerIdCodec.toHex(ad.sig),
                    createdAtMs = System.currentTimeMillis(),
                    verified = when (outcome) {
                        iriscode.FfiAdoptionOutcome.REFRESHED,
                        iriscode.FfiAdoptionOutcome.DUPLICATE -> existing?.verified == true
                        else -> false
                    },
                )
            }.isSuccess
            if (!persisted) return AddPeerOutcome.PersistenceFailed
        }
        return AddPeerOutcome.Adopted(peerIdHex, outcome)
    }

    /** Trusted-peers: the Short Authentication String between this node and `peerIdHex`. */
    fun fingerprintFor(peerIdHex: String): String? {
        val identity = PeerIdCodec.fromHex(peerIdHex) ?: return null
        return runCatching { engine.get().computeSas(identity) }.getOrNull()
    }

    /**
     * Trusted-peers: out-of-band confirmation — promotes `peerIdHex` to
     * `Verified` using the X25519 key from its last-adopted advertisement.
     * @return `false` if there is no record for this peer, or the engine
     * rejected the confirmation (RT-010: the key on file no longer matches
     * what was most recently adopted — re-run `/addkey` with a fresh code).
     */
    fun confirmPeer(peerIdHex: String): Boolean {
        val record = trustedPeers.get(peerIdHex) ?: return false
        val identity = PeerIdCodec.fromHex(peerIdHex) ?: return false
        val x25519 = PeerIdCodec.fromHex(record.x25519Hex) ?: return false
        return try {
            engine.get().verifyPeer(identity, x25519)
            nsdAdapter.onPeerTrusted(peerIdHex)
            trustedPeers.markVerified(peerIdHex)
            scheduleInternetActivation(peerIdHex)
            true
        } catch (_: IrisFfiException) {
            false
        }
    }

    fun myPairingCode(): String? = PairingCodeCodec.forQr(myAdvertisement())

    fun persistedVerifiedPeerIds(): Set<String> = trustedPeers.all()
        .filterValues { it.verified }
        .keys

    fun isPersistedVerifiedPeer(peerIdHex: String): Boolean =
        trustedPeers.get(peerIdHex)?.verified == true

    /** Trusted-peers: current trust level, or `null` if this peer id is malformed. */
    fun trustLevelFor(peerIdHex: String): iriscode.FfiTrustLevel? {
        val identity = PeerIdCodec.fromHex(peerIdHex) ?: return null
        return runCatching { engine.get().trustLevel(identity) }.getOrNull()
    }

    /** Trusted-peers: drop a peer's persisted trusted-key record. */
    suspend fun forgetTrustedPeer(peerIdHex: String) {
        activationJobs.remove(peerIdHex)?.cancel()
        val identity = PeerIdCodec.fromHex(peerIdHex)
        if (identity != null) runCatching { engine.get().forgetPeer(identity) }
        trustedPeers.remove(peerIdHex)
        knownPeers.remove(peerIdHex)
        outbox.removeRecipient(peerIdHex)
    }

    /** Spill the relay outbox into the engine (WorkManager entrypoint). */
    suspend fun drainRelayOutbox(): Int {
        val sent = outbox.drain { queued ->
            try {
                engine.get().sendText(queued.recipientHex, queued.text, queued.priority)
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

        /** HV-41: display label for a sent broadcast — not a real PeerId. */
        const val BROADCAST_LABEL = "BROADCAST"

        /** Ceiling on retained UI messages; the engine holds the durable copy. */
        const val MAX_UI_MESSAGES = 500

        private const val INTERNET_RETRY_INITIAL_MS = 1_000L
        private const val INTERNET_RETRY_MAX_MS = 30_000L

        private fun FfiIncomingMessage.toUi(): InboxUiMessage = InboxUiMessage.received(
            senderId = PeerIdCodec.toHex(senderId),
            payloadUtf8 = payload.toString(Charsets.UTF_8),
            priority = priority,
            receivedAtMs = receivedAtMs.toLong(),
        )
    }
}
