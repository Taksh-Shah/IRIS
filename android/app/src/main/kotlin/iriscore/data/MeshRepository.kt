package iriscore.data

import android.util.Log
import dagger.Lazy
import iriscode.FfiInboxListener
import iriscode.FfiIncomingMessage
import iriscode.IrisEngine
import iriscode.IrisFfiException
import iriscore.adapter.AndroidNsdAdapter
import iriscore.adapter.AndroidInternetNetworkMonitor
import iriscore.data.db.MessageDao
import iriscore.data.db.MessageEntity
import iriscore.di.DefaultDispatcher
import iriscore.di.NodeId
import iriscore.identity.KeystoreEd25519
import iriscore.ui.state.DeliveryStatus
import iriscore.ui.state.InboxUiMessage
import iriscore.ui.state.MeshStatus
import iriscore.ui.state.MeshUiState
import iriscore.ui.state.TransportStatus
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
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Clean-architecture repository bridging the Rust [IrisEngine] to the UI
 * (AC-6 MVVM) and the WorkManager relay path (AC-7).
 *
 * WP2: messages and pending sends are now durable. [messageDao] is the
 * source of truth for the message list — [uiState.messages] is derived from
 * its [MessageDao.observeRecent] Flow so history survives process death and
 * reboots. [outbox] is backed by [iriscore.data.db.PendingMessageDao] for the
 * same reason.
 */
@Singleton
class MeshRepository @Inject constructor(
    private val engine: Lazy<IrisEngine>,
    @NodeId private val nodeId: ByteArray,
    private val outbox: RelayOutbox,
    private val messageDao: MessageDao,
    private val keystore: KeystoreEd25519,
    private val knownPeers: KnownPeersStore,
    private val trustedPeers: TrustedPeerStore,
    private val nsdAdapter: AndroidNsdAdapter,
    private val internetMonitor: AndroidInternetNetworkMonitor,
    @DefaultDispatcher private val dispatcher: CoroutineDispatcher,
) {
    private val scope = CoroutineScope(SupervisorJob() + dispatcher)

    private val inboxSubscribed = AtomicBoolean(false)
    private val meshRequested = AtomicBoolean(false)
    private val activationJobs = ConcurrentHashMap<String, Job>()
    private val activationMutex = Mutex()
    private val lifecycleLock = Any()

    /**
     * Maps `rustMsgIdHex → uid` for in-flight sent messages awaiting a
     * delivery ACK from the engine. Populated by [send] and [broadcast];
     * consumed by [onDeliveryAck] when the Rust side fires the callback.
     */
    private val pendingAcks = ConcurrentHashMap<String, Long>()

    private val nodeIdHex = PeerIdCodec.toHex(nodeId)

    // WP2: uiState is now derived from Room Flows so the message list and
    // relay-queued count survive process death. Non-message fields (status,
    // error) remain in-memory MutableStateFlows that are reset on each start.
    private val _meshStatus = MutableStateFlow(MeshStatus.IDLE)
    private val _lastError = MutableStateFlow<String?>(null)

    val uiState: StateFlow<MeshUiState> = combine(
        messageDao.observeRecent(),
        outbox.observeCount(),
        _meshStatus,
        _lastError,
    ) { messages, pendingCount, status, error ->
        MeshUiState(
            nodeIdHex = nodeIdHex,
            nodeIdShort = nodeIdHex.take(16),
            identityBackend = keystore.backendType.name,
            status = status,
            messages = messages.map { it.toUiMessage() },
            relayQueued = pendingCount,
            lastError = error,
        )
    }.stateIn(
        scope,
        SharingStarted.WhileSubscribed(5_000),
        MeshUiState.initial(nodeIdHex, keystore.backendType.name),
    )

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

    private fun reconcileMeshState() {
        if (!meshRequested.get()) return
        runCatching { engine.get().startAll() }
            .onSuccess { _meshStatus.value = MeshStatus.RUNNING }
            .onFailure { error ->
                Log.i(TAG, "Mesh is waiting for a usable transport: ${error.message}")
                _meshStatus.value = MeshStatus.UNAVAILABLE
            }
    }

    fun startMesh() {
        synchronized(lifecycleLock) {
            meshRequested.set(true)
            try {
                for ((peerHex, x25519Hex) in knownPeers.all()) {
                    runCatching { engine.get().registerPeerKey(peerHex, x25519Hex) }
                }
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
                internetMonitor.start()
                val network = internetMonitor.currentState()
                engine.get().setInternetNetworkState(network.validatedWan, network.localNetwork)
                reconcileMeshState()
                if (network.localNetwork) nsdAdapter.start()
                activateVerifiedInternetPeers()
            } catch (e: Exception) {
                Log.w(TAG, "startMesh: engine.startAll() failed", e)
                _meshStatus.value = MeshStatus.UNAVAILABLE
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
            } finally {
                _meshStatus.value = MeshStatus.IDLE
            }
        }
    }

    /**
     * Install the inbox listener; idempotent — duplicate registrations on
     * screen rotation are silently dropped by the AtomicBoolean guard.
     *
     * WP2: received messages are written to Room so they survive process death.
     */
    // TODO(WP4): when Rust adds FfiDeliveryListener, subscribe here and call
    // onDeliveryAck(rustMsgIdHex) for each fired ACK so the Room row transitions
    // from DELIVERED (best-effort) to a confirmed state.
    fun subscribeInbox() {
        if (!inboxSubscribed.compareAndSet(false, true)) return
        engine.get().subscribeInbox(object : FfiInboxListener {
            override fun onMessage(message: FfiIncomingMessage) {
                val ui = message.toUi()
                scope.launch {
                    messageDao.insert(MessageEntity.fromUiMessage(ui))
                }
            }
        })
    }

    /**
     * Called when the Rust engine fires a delivery acknowledgment for a message
     * this node sent. Upgrades the Room row from the optimistic DELIVERED status
     * written at send time to a confirmed state once real ACKs land.
     *
     * Wire this up to `FfiDeliveryListener.onDelivery` once that callback is
     * added to the FFI — see TODO(WP4) in [subscribeInbox].
     */
    private fun onDeliveryAck(rustMsgIdHex: String) {
        val uid = pendingAcks.remove(rustMsgIdHex) ?: return
        scope.launch { messageDao.updateStatus(uid, DeliveryStatus.DELIVERED.name) }
    }

    fun send(recipientHex: String, text: String, priority: UByte): Boolean {
        val normalized = recipientHex.trim().lowercase()
        if (!PeerIdCodec.isHex(normalized) || normalized.length != RECIPIENT_HEX_LENGTH) {
            _lastError.value = "Recipient must be a $RECIPIENT_HEX_LENGTH-character hex PeerId"
            return false
        }
        _lastError.value = null
        return try {
            val rustMsgId = engine.get().sendText(normalized, text, priority)
            val sent = InboxUiMessage.sent(normalized, text, priority)
            pendingAcks[rustMsgId.joinToString("") { "%02x".format(it) }] = sent.uid
            scope.launch { messageDao.insert(MessageEntity.fromUiMessage(sent)) }
            true
        } catch (_: IrisFfiException) {
            scope.launch {
                val pending = InboxUiMessage.pending(normalized, text, priority)
                messageDao.insert(MessageEntity.fromUiMessage(pending))
                // WP11: the pending-outbox row now carries the same uid as the
                // message row above, so a later successful drain (or a
                // permanent give-up) can update THIS row instead of leaving
                // it stuck at QUEUED forever — see drainRelayOutbox().
                outbox.enqueue(RelayOutbox.QueuedMessage(pending.uid, normalized, text, priority))
            }
            false
        }
    }

    fun broadcast(text: String, priority: UByte): Boolean {
        _lastError.value = null
        return try {
            val rustMsgId = engine.get().broadcastText(text, priority)
            val sent = InboxUiMessage.sent(BROADCAST_LABEL, text, priority)
            pendingAcks[rustMsgId.joinToString("") { "%02x".format(it) }] = sent.uid
            scope.launch { messageDao.insert(MessageEntity.fromUiMessage(sent)) }
            true
        } catch (e: IrisFfiException) {
            _lastError.value = "Broadcast failed: ${e.message}"
            false
        }
    }

    fun snapshot(): iriscode.FfiMeshSnapshot = engine.get().snapshot()

    fun transportStatuses(): List<TransportStatus> =
        engine.get().snapshot().transports.map { t ->
            TransportStatus(
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

    fun staticX25519(): String =
        engine.get().staticX25519Pubkey().joinToString("") { "%02x".format(it) }

    fun addPeerKey(peerHex: String, x25519Hex: String) {
        engine.get().registerPeerKey(peerHex, x25519Hex)
        runCatching { knownPeers.put(peerHex, x25519Hex) }
    }

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

    sealed interface AddPeerOutcome {
        data class Adopted(val peerIdHex: String, val outcome: iriscode.FfiAdoptionOutcome) : AddPeerOutcome
        data object InvalidBlob : AddPeerOutcome
        data object SelfPairing : AddPeerOutcome
        data object PersistenceFailed : AddPeerOutcome
    }

    fun addTrustedPeer(blob: String): AddPeerOutcome {
        val canonical = PairingCodeCodec.toAdvertisementHex(blob) ?: return AddPeerOutcome.InvalidBlob
        val ad = AdvertisementCodec.decode(canonical) ?: return AddPeerOutcome.InvalidBlob
        if (ad.identityPubkey.contentEquals(nodeId)) return AddPeerOutcome.SelfPairing
        val existing = trustedPeers.get(PeerIdCodec.toHex(ad.identityPubkey))
        val outcome = try {
            engine.get().adoptPeerAdvertisement(ad)
        } catch (_: IrisFfiException) {
            return AddPeerOutcome.InvalidBlob
        }
        val peerIdHex = PeerIdCodec.toHex(ad.identityPubkey)
        val adoptedAsCanonical = when (outcome) {
            iriscode.FfiAdoptionOutcome.BOUND_UNVERIFIED,
            iriscode.FfiAdoptionOutcome.REFRESHED,
            iriscode.FfiAdoptionOutcome.ROTATION_ADOPTED -> true
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

    fun fingerprintFor(peerIdHex: String): String? {
        val identity = PeerIdCodec.fromHex(peerIdHex) ?: return null
        return runCatching { engine.get().computeSas(identity) }.getOrNull()
    }

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

    fun trustLevelFor(peerIdHex: String): iriscode.FfiTrustLevel? {
        val identity = PeerIdCodec.fromHex(peerIdHex) ?: return null
        return runCatching { engine.get().trustLevel(identity) }.getOrNull()
    }

    suspend fun forgetTrustedPeer(peerIdHex: String) {
        activationJobs.remove(peerIdHex)?.cancel()
        val identity = PeerIdCodec.fromHex(peerIdHex)
        if (identity != null) runCatching { engine.get().forgetPeer(identity) }
        trustedPeers.remove(peerIdHex)
        knownPeers.remove(peerIdHex)
        outbox.removeRecipient(peerIdHex)
    }

    /**
     * WP11: drain the outbox, updating the real message row on every outcome
     * instead of leaving it stuck at QUEUED forever. A successful send flips
     * that row to DELIVERED (best-effort local evidence, same convention as
     * [send]'s own accept path); a row that has exhausted
     * [RelayOutbox.MAX_DRAIN_ATTEMPTS] flips to FAILED and stops being retried.
     */
    suspend fun drainRelayOutbox(): Int {
        val sent = outbox.drain(
            onSend = { queued ->
                try {
                    val rustMsgId = engine.get().sendText(queued.recipientHex, queued.text, queued.priority)
                    pendingAcks[rustMsgId.joinToString("") { "%02x".format(it) }] = queued.messageUid
                    messageDao.updateStatus(queued.messageUid, DeliveryStatus.DELIVERED.name)
                    true
                } catch (_: IrisFfiException) {
                    false
                }
            },
            onGiveUp = { messageUid ->
                messageDao.updateStatus(messageUid, DeliveryStatus.FAILED.name)
            },
        )
        return sent
    }

    val pendingRelayCount: Int get() = outbox.size

    /**
     * WP12: re-attempt a FAILED outbound message, reusing its existing row
     * (per the "every transition updates the same persistent message row"
     * acceptance gate — a retry must never spawn a second row for the same
     * message). Only meaningful for outbound rows; returns false for an
     * unknown uid or an inbound row.
     */
    suspend fun retry(uid: Long): Boolean {
        val row = messageDao.getById(uid) ?: return false
        if (!row.isOutbound) return false
        return try {
            val rustMsgId = engine.get().sendText(row.peerIdHex, row.payloadUtf8, row.priority.toUByte())
            pendingAcks[rustMsgId.joinToString("") { "%02x".format(it) }] = uid
            messageDao.updateStatus(uid, DeliveryStatus.DELIVERED.name)
            true
        } catch (_: IrisFfiException) {
            outbox.enqueue(RelayOutbox.QueuedMessage(uid, row.peerIdHex, row.payloadUtf8, row.priority.toUByte()))
            messageDao.updateStatus(uid, DeliveryStatus.QUEUED.name)
            false
        }
    }

    /**
     * WP12: local-only deletion — never reaches the sender or the mesh.
     * Also drops any outbox row still queued for this exact message (by uid,
     * not by recipient — [RelayOutbox.removeRecipient] would wrongly also
     * drop other pending sends to the same peer).
     */
    suspend fun deleteMessage(uid: Long) {
        messageDao.delete(uid)
        outbox.removeByMessageUid(uid)
    }

    companion object {
        private const val TAG = "IrisMeshRepository"
        const val RECIPIENT_HEX_LENGTH = 64
        const val BROADCAST_LABEL = "BROADCAST"

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
