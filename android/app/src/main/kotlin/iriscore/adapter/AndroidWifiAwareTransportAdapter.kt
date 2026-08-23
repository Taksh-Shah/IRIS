package iriscore.adapter

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.wifi.aware.AttachCallback
import android.net.wifi.aware.DiscoverySession
import android.net.wifi.aware.DiscoverySessionCallback
import android.net.wifi.aware.IdentityChangedListener
import android.net.wifi.aware.PeerHandle
import android.net.wifi.aware.PublishConfig
import android.net.wifi.aware.PublishDiscoverySession
import android.net.wifi.aware.SubscribeConfig
import android.net.wifi.aware.SubscribeDiscoverySession
import android.net.wifi.aware.WifiAwareManager
import android.net.wifi.aware.WifiAwareNetworkInfo
import android.net.wifi.aware.WifiAwareNetworkSpecifier
import android.net.wifi.aware.WifiAwareSession
import java.net.Inet6Address
import iriscode.DeviceNotFound
import iriscode.FfiIncomingNdpData
import iriscode.FfiPeerDiscovery
import iriscode.FfiPublishConfig
import iriscode.FfiWifiAwareAdapter
import iriscode.TransportFailure
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicLong
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.CancellableContinuation
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout

/**
 * 12-op async `FfiWifiAwareAdapter` foreign-trait implementation.
 *
 * Maps the core WIFIAWARE-001 FFI contract onto the Android Wi-Fi Aware APIs
 * and absorbs the AC-5 lifecycle requirements:
 *
 *  - [SessionGate] (NEW-WA-RT-111 / RT-010): one attach / subscribe / publish
 *    session per adapter lifetime, reused on every re-entry; a failed platform
 *    call clears the latch so the next start retries fresh (never latched into
 *    failure). Discovery sessions seeded via `onSessionStarted` ->
 *    [SessionGate.markStarted] (async platform delivery).
 *  - [NdpRegistry] (NEW-WA-RT-109): open/close NDP tracking + closed-NDP
 *    prune at the drain boundary; availability is a multi-subscriber replaying
 *    stream ([RadioStateTracker]) rather than the core's one-shot take().
 *  - [VerifiedPeerCache] (NEW-WA-RT-108 / DEC-WA-0007): candidate `PeerHandle`
 *    key switches to the VERIFIED 64-hex PeerId once a real envelope resolves.
 *  - [RingBufferOutbox] (NEW-WA-RT-112): bounded, oldest-drop, per-destination
 *    eviction for the NDP send path until a live socket drains it.
 *  - [FfiCallTimeout] (NEW-WA-RT-110): every suspend op is time-boxed.
 *  - `isAvailable()` stays the pull-only projection for the Rust bridge
 *    (proj-WA-1); the platform push channel feeds [RadioStateTracker].
 *
 * NOTE: Gradle/Android compile + device legs are ENV-GATED on the integration
 * host (no Android SDK/kotlinc); this source is the AC-5 deliverable and is
 * verified by the CI/Gradle leg (AC-6/AC-11). The NDP socket data-path
 * handshake (NetworkCallback.onAvailable -> ServerSocket accept / Socket
 * connect over the NDP IPv6 link) is platform plumbing deferred to AC-6.
 */
class AndroidWifiAwareTransportAdapter(context: Context) : FfiWifiAwareAdapter {

    companion object {
        /** NAN service name for the IRIS mesh (WIFI_AWARE.md §4). */
        const val IRIS_SERVICE_NAME = "com.iris.mesh.v1"
    }

    private val appContext: Context = context.applicationContext
    private val awareManager: WifiAwareManager =
        appContext.getSystemService(Context.WIFI_AWARE_SERVICE) as WifiAwareManager

    private val availability = RadioStateTracker()
    private val ndpRegistry = NdpRegistry()
    private val verifiedCache = VerifiedPeerCache()

    /** Frames queued for an NDP whose socket is not up yet; flushed on connect. */
    private val outbox = RingBufferOutbox<ByteArray>()

    /**
     * Frames genuinely received from peers. Strictly separate from [outbox] —
     * draining the outbox on the inbound path made the adapter echo its own
     * sends back to the core as peer traffic.
     */
    private val inbox = RingBufferOutbox<ByteArray>()

    private val links = SocketLinkRegistry()

    // AND-RT-104: attach() awaits the platform `onAttached` session; the gate
    // caches it so `ensureStarted` never re-attaches.
    private val attachGate = SessionGate<WifiAwareSession> { attach() }
    private val subscribeGate = SessionGate<SubscribeDiscoverySession?> { subscribeOnce() }
    private val publishGate = SessionGate<PublishDiscoverySession?> { publishOnce() }

    /** Lifts non-suspend platform callbacks into suspend [SessionGate] calls. */
    private val callbackScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    private val nextHandle = AtomicLong(1L)
    private val peerHandles = ConcurrentHashMap<Long, PeerHandle>()
    private val ndpSpecifiers = ConcurrentHashMap<Long, WifiAwareNetworkSpecifier>()
    private val peerByNdp = ConcurrentHashMap<Long, Long>() // NDP handle -> peer (platform) handle
    private val ndpCallbacks = ConcurrentHashMap<Long, ConnectivityManager.NetworkCallback>()
    private val discoveredMatches = CopyOnWriteArrayList<FfiPeerDiscovery>()
    private val inboundFrames = MutableSharedFlow<FfiIncomingNdpData>(extraBufferCapacity = 256)

    override suspend fun start() {
        FfiCallTimeout.suspendCall {
            attachGate.ensureStarted()
            availability.setAvailable(awareManager.isAvailable)
        }
    }

    override suspend fun subscribe() {
        FfiCallTimeout.suspendCall { subscribeGate.ensureStarted() }
    }

    override suspend fun unsubscribe() {
        FfiCallTimeout.suspendCall { subscribeGate.reset() }
    }

    override suspend fun publish(config: FfiPublishConfig) {
        FfiCallTimeout.suspendCall { publishGate.ensureStarted() }
    }

    override suspend fun unpublish() {
        FfiCallTimeout.suspendCall { publishGate.reset() }
    }

    override suspend fun matches(): List<FfiPeerDiscovery> {
        return FfiCallTimeout.suspendCall {
            val drained = discoveredMatches.toList()
            discoveredMatches.clear()
            drained
        }
    }

    override suspend fun openNdp(peerHandle: ULong): ULong {
        return FfiCallTimeout.suspendCall {
            attachGate.ensureStarted()
            // The specifier is built from the DISCOVERY session, not the attach
            // session: a PeerHandle is only meaningful within the discovery
            // session that surfaced it. The subscribe session is seeded
            // asynchronously by onSubscribeStarted, so it may not be live yet.
            val discovery = subscribeGate.ensureStarted()
                ?: throw TransportFailure("Wi-Fi Aware discovery session not started")
            val peer = peerHandles[peerHandle.toLong()]
                ?: throw DeviceNotFound()
            val specifier = WifiAwareNetworkSpecifier.Builder(discovery, peer)
                .setPskPassphrase(NDP_PSK_PASSPHRASE) // NCS passphrase hardening (WFA 4.0)
                .build()
            val ndpHandle = nextHandle.getAndIncrement()
            // AND-RT-110: NOT registered as opened here — the handle becomes
            // enqueue-able only once its network is genuinely available
            // (onAvailable).
            peerByNdp[ndpHandle] = peerHandle.toLong()
            ndpSpecifiers[ndpHandle] = specifier
            requestAwareNetwork(specifier, ndpHandle)
            ndpHandle.toULong()
        }
    }

    override suspend fun closeNdp(ndpHandle: ULong) {
        FfiCallTimeout.suspendCall {
            ndpRegistry.closed(ndpHandle.toLong())
            ndpSpecifiers.remove(ndpHandle.toLong())
            links.remove(ndpHandle.toLong())
            // AND-RT-109: closed NDPs must not retain frames.
            outbox.drainDestination(ndpHandle.toLong())
            inbox.drainDestination(ndpHandle.toLong())
            ndpCallbacks.remove(ndpHandle.toLong())?.let { cb ->
                runCatching { connectivityManager.unregisterNetworkCallback(cb) }
            }
            peerByNdp.remove(ndpHandle.toLong())?.let { verifiedCache.evict(it) }
        }
    }

    override suspend fun ndpSend(ndpHandle: ULong, payload: ByteArray) {
        FfiCallTimeout.suspendCall {
            // AND-RT-110: only enqueue once the NDP network is genuinely available.
            val handle = ndpHandle.toLong()
            if (!ndpRegistry.accepts(handle)) {
                throw TransportFailure("NDP network not yet available")
            }
            val link = links.get(handle)
            if (link != null && link.send(payload)) return@suspendCall
            // Network is up but the socket is still being established (the peer
            // address arrives on onCapabilitiesChanged): hold and flush later.
            outbox.enqueue(handle, payload)
        }
    }

    override suspend fun incomingNdp(): List<FfiIncomingNdpData> {
        return FfiCallTimeout.suspendCall {
            val drained = mutableListOf<FfiIncomingNdpData>()
            // Closed-NDP prune (RT-109): only frames for OPEN ndps are surfaced.
            for (handle in ndpRegistry.openHandles()) {
                val peerHandle = peerByNdp[handle] ?: continue
                val sender = verifiedCache.verifiedPeerIdFor(peerHandle)
                for (frame in inbox.drain(handle)) {
                    drained.add(
                        FfiIncomingNdpData(
                            sender = sender?.hexToBytes(),
                            ndpHandle = handle.toULong(),
                            payload = frame,
                        ),
                    )
                }
            }
            drained
        }
    }

    override suspend fun shutdown() {
        FfiCallTimeout.suspendCall {
            pruneNdpRegistrations()
            peerHandles.keys.forEach { verifiedCache.evict(it) }
            peerHandles.clear()
            subscribeGate.reset()
            publishGate.reset()
            attachGate.reset()
            availability.setAvailable(false)
        }
    }

    override fun isAvailable(): Boolean =
        FfiCallTimeout.syncCall(onTimeout = false) { availability.isAvailable() }

    // -- platform plumbing -------------------------------------------------

    private val connectivityManager: ConnectivityManager =
        appContext.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager

    private var pendingAttach: CancellableContinuation<WifiAwareSession>? = null

    private val identityListener = object : IdentityChangedListener() {
        override fun onIdentityChanged(byte: ByteArray?) = Unit // NAN MAC randomization is built-in
    }

    /**
     * AND-RT-104: `WifiAwareManager.attach` returns Unit — the session arrives
     * asynchronously via [WifiAwareManager.AttachCallback.onAttached]. This
     * awaits that callback under the RT-110 timeout; the [SessionGate] then
     * caches the session so `ensureStarted` never re-attaches.
     */
    private suspend fun attach(): WifiAwareSession = withTimeout(FfiCallTimeout.SHORT_TIMEOUT_MS) {
        suspendCancellableCoroutine { cont ->
            cont.invokeOnCancellation { pendingAttach = null }
            pendingAttach = cont
            runCatching {
                // Platform order is (AttachCallback, IdentityChangedListener,
                // Handler?) — the listener is non-null in that overload.
                awareManager.attach(attachCallback, identityListener, null)
            }.getOrElse { e ->
                pendingAttach = null
                cont.resumeWithException(TransportFailure("Wi-Fi Aware attach failed: ${e.message}"))
            }
        }
    }

    private val attachCallback = object : AttachCallback() {
        override fun onAttached(session: WifiAwareSession) {
            availability.setAvailable(true)
            pendingAttach?.let { cont ->
                pendingAttach = null
                cont.resume(session)
            }
        }

        override fun onAttachFailed() {
            availability.setAvailable(false)
            pendingAttach?.let { cont ->
                pendingAttach = null
                cont.resumeWithException(TransportFailure("Wi-Fi Aware attach failed"))
            }
        }
    }

    private suspend fun subscribeOnce(): SubscribeDiscoverySession? {
        val session = attachGate.ensureStarted()
        session.subscribe(subscribeConfig(), discoveryCallback, null)
        return null // live resource seeded by onSessionStarted (async)
    }

    private suspend fun publishOnce(): PublishDiscoverySession? {
        val session = attachGate.ensureStarted()
        session.publish(publishConfig(), discoveryCallback, null)
        return null // live resource seeded by onSessionStarted (async)
    }

    private fun subscribeConfig(): SubscribeConfig =
        SubscribeConfig.Builder()
            .setServiceName(IRIS_SERVICE_NAME)
            .build()

    private fun publishConfig(): PublishConfig =
        PublishConfig.Builder()
            .setServiceName(IRIS_SERVICE_NAME)
            .build()

    private val discoveryCallback = object : DiscoverySessionCallback() {
        override fun onServiceDiscovered(
            peerHandle: PeerHandle,
            serviceSpecificInfo: ByteArray?,
            matchFilter: List<ByteArray>?,
        ) {
            val handle = nextHandle.getAndIncrement()
            peerHandles[handle] = peerHandle
            // AND-RT-108: remember the candidate beacon identity (DEC-WA-0007) so
            // inbound frames can be attributed to a 64-hex PeerId.
            beaconCandidatePeerIdHex(serviceSpecificInfo ?: ByteArray(0))?.let {
                verifiedCache.rememberVerified(handle, it)
            }
            discoveredMatches.add(
                FfiPeerDiscovery(
                    peerHandle = handle.toULong(),
                    serviceSpecificInfo = serviceSpecificInfo ?: ByteArray(0),
                    rssi = 0, // NAN service discovery does not surface rssi on this API level
                ),
            )
        }

        // The platform has no single `onSessionStarted` — publish and subscribe
        // sessions are delivered on their own typed callbacks.
        // markStarted is suspend (AND-RT-105); lift the platform callback.
        override fun onSubscribeStarted(session: SubscribeDiscoverySession) {
            callbackScope.launch { subscribeGate.markStarted(session) }
        }

        override fun onPublishStarted(session: PublishDiscoverySession) {
            callbackScope.launch { publishGate.markStarted(session) }
        }

        override fun onSessionConfigFailed() = Unit

        override fun onSessionTerminated() {
            // AND-RT-110: prune every registered peer handle + specifier +
            // open NDP so a dead session leaks nothing.
            pruneNdpRegistrations()
            peerHandles.keys.forEach { verifiedCache.evict(it) }
            peerHandles.clear()
            callbackScope.launch {
                subscribeGate.invalidate()
                publishGate.invalidate()
                attachGate.invalidate()
            }
        }
    }

    /** Opens the NDP NetworkRequest for the given specifier (socket handshake at AC-6). */
    private fun requestAwareNetwork(specifier: WifiAwareNetworkSpecifier, ndpHandle: Long) {
        val request = NetworkRequest.Builder()
            .addTransportType(NetworkCapabilities.TRANSPORT_WIFI_AWARE)
            .setNetworkSpecifier(specifier)
            .build()
        val callback = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                // AND-RT-110: an NDP handle is only enqueue-able once its network
                // is genuinely available.
                ndpRegistry.opened(ndpHandle)
            }

            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                // The peer's link-local IPv6 address and port are only published
                // here, via WifiAwareNetworkInfo — onAvailable does not carry
                // them, so this is where the data socket can first be dialled.
                val info = caps.transportInfo as? WifiAwareNetworkInfo ?: return
                val peerAddress = info.peerIpv6Addr ?: return
                if (info.port <= 0) return
                callbackScope.launch {
                    connectNdpSocket(network, peerAddress, info.port, ndpHandle)
                }
            }

            override fun onLost(network: Network) {
                ndpRegistry.closed(ndpHandle)
                links.remove(ndpHandle)
            }
        }
        ndpCallbacks[ndpHandle] = callback
        connectivityManager.requestNetwork(request, callback)
    }

    /**
     * Dials the peer over the NDP link and registers the resulting frame link.
     *
     * The socket MUST be created from the NDP [Network]'s own socket factory:
     * the peer address is link-local IPv6 and is only routable on that network,
     * so a default-network socket would fail or leak onto the wrong interface.
     */
    private suspend fun connectNdpSocket(
        network: Network,
        peerAddress: Inet6Address,
        port: Int,
        ndpHandle: Long,
    ) {
        if (links.get(ndpHandle) != null) return
        val socket = withContext(Dispatchers.IO) {
            runCatching {
                network.socketFactory.createSocket(peerAddress, port)
            }.getOrNull()
        } ?: return

        val link = FramedSocketLink(
            socket = socket,
            scope = callbackScope,
            onFrame = { frame -> inbox.enqueue(ndpHandle, frame) },
            onClosed = { links.remove(ndpHandle) },
        )
        links.put(ndpHandle, link)
        // Flush whatever was queued while the socket was coming up.
        for (frame in outbox.drain(ndpHandle)) {
            if (!link.send(frame)) {
                outbox.enqueue(ndpHandle, frame)
                break
            }
        }
    }

    /** Prunes all registered NDP state (AND-RT-110/109): registry, specifiers,
     *  callback registrations, outbox + verified-cache bindings. */
    private fun pruneNdpRegistrations() {
        ndpRegistry.closeAll().forEach { handle ->
            ndpSpecifiers.remove(handle)
            links.remove(handle)
            ndpCallbacks.remove(handle)?.let { cb ->
                runCatching { connectivityManager.unregisterNetworkCallback(cb) }
            }
            peerByNdp.remove(handle)?.let { verifiedCache.evict(it) }
            outbox.drainDestination(handle)
            inbox.drainDestination(handle)
        }
    }

    private fun String.hexToBytes(): ByteArray {
        if (length % 2 != 0) return ByteArray(0)
        return ByteArray(length / 2) { i -> substring(i * 2, i * 2 + 2).toInt(16).toByte() }
    }
}

/**
 * NCS passphrase for the optional WFA 4.0-aware NDP hardening. Generated per
 * session by the Keystore identity path (AC-8); constant placeholder for AC-5.
 */
internal const val NDP_PSK_PASSPHRASE = "iris-ndp-psk"