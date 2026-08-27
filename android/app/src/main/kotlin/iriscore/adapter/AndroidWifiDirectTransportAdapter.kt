package iriscore.adapter

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.wifi.p2p.WifiP2pConfig
import android.net.wifi.p2p.WifiP2pGroup
import android.net.wifi.p2p.WifiP2pInfo
import android.net.wifi.p2p.WifiP2pManager
import android.net.wifi.p2p.nsd.WifiP2pDnsSdServiceInfo
import android.net.wifi.p2p.nsd.WifiP2pDnsSdServiceRequest
import android.os.Build
import android.os.Looper
import android.os.SystemClock
import androidx.core.content.ContextCompat
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.Socket
import java.security.SecureRandom
import iriscode.FfiDirectPeerDiscovery
import iriscode.FfiGroupConfig
import iriscode.FfiGroupInfo
import iriscode.FfiIncomingWifiDirectData
import iriscode.FfiOperatingBand
import iriscode.DeviceNotFound
import iriscode.NotSupported
import iriscode.FfiWifiDirectAdapter
import iriscode.TransportFailure
import iriscore.util.PeerIdCodec
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicLong
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext

/**
 * 20-op async `FfiWifiDirectAdapter` foreign-trait implementation.
 *
 * Maps the WIFIDIRECT-001 FFI contract (RES-0021 Q4/Q5: Android
 * `WifiP2pManager` DNS-SD discovery + `createGroup` persistent GO) onto the
 * platform APIs, absorbing the AC-5 lifecycle requirements:
 *
 *  - [SessionGate] (NEW-WA-RT-111 / WIFIDIRECT RT-010): `start`/`startDnsSd`
 *    are idempotent — one `Channel` + one DNS-SD advertisement per adapter
 *    lifetime; a failed platform call clears the latch (never latched into
 *    failure). This mirrors the transport-side `ensure_started()` fix; a lost
 *    channel calls [SessionGate.invalidate] from the platform callback.
 *  - [RingBufferOutbox] (NEW-WA-RT-112): bounded, oldest-drop, per-peer
 *    eviction for `p2pSend` until the TCP-over-GO socket path drains it
 *    (socket data path = AC-6 platform plumbing).
 *  - [RadioStateTracker] + [NdpRegistry] (RT-109): availability push stream +
 *    closed-group prune; `isAvailable()` stays pull-only (proj-WA-1).
 *  - [VerifiedPeerCache] (RT-108): candidate P2P device key -> VERIFIED PeerId.
 *  - [FfiCallTimeout] (RT-110): every suspend op is time-boxed.
 *
 * Band mapping (`setGroupOperatingBand`, API 29+): AUTO = platform default
 * (no-op); GHZ24/GHZ5/GHZ6 map to `WifiP2pManager.BAND_*`.
 *
 * NOTE: Gradle/Android compile + device legs are ENV-GATED on the integration
 * host (no Android SDK/kotlinc); source is the AC-5 deliverable, verified by
 * the CI/Gradle leg (AC-6/AC-11).
 */
class AndroidWifiDirectTransportAdapter(context: Context) : FfiWifiDirectAdapter {

    companion object {
        /** DNS-SD service type (Bonjour) — RES-0021 Q4. */
        const val DNS_SD_SERVICE_TYPE = "_iris._tcp"

        /**
         * TXT-record key the 22-byte IRIS beacon (hex-encoded) is published
         * under (FFI-5). DNS-SD TXT values are text; hex is the binary-safe
         * encoding both `registerDnsSd` and `encodeTxtRecord` agree on.
         */
        const val BEACON_TXT_KEY = "b"

        /** Bounded settle window for the async CONNECTION_CHANGED group snapshot. */
        private const val GROUP_INFO_SETTLE_MS = 5_000L

        /** Poll interval while awaiting the group snapshot (AND-RT-103). */
        private const val GROUP_INFO_POLL_MS = 100L

        /** Bounded dial timeout for the GO socket; never blocks the callback scope. */
        private const val SOCKET_CONNECT_TIMEOUT_MS = 10_000
    }

    private val appContext: Context = context.applicationContext
    /**
     * Null when the device has no Wi-Fi Direct support. The cast used to be
     * non-null, throwing at Hilt injection on the main thread.
     */
    private val p2pManager: WifiP2pManager? =
        appContext.getSystemService(Context.WIFI_P2P_SERVICE) as? WifiP2pManager

    /** Lifts non-suspend platform callbacks into suspend [SessionGate] calls. */
    private val callbackScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    private val availability = RadioStateTracker()
    private val groupRegistry = NdpRegistry()
    private val verifiedCache = VerifiedPeerCache()

    /** Frames queued for a peer whose socket is not up yet; flushed on connect. */
    private val outbox = RingBufferOutbox<ByteArray>()

    /**
     * Frames genuinely received from peers. Strictly separate from [outbox] —
     * draining the outbox on the inbound path made the adapter echo its own
     * sends back to the core as peer traffic.
     */
    private val inbox = RingBufferOutbox<ByteArray>()

    private val links = SocketLinkRegistry()

    /** GO-side accept loop; null while this node is not the group owner. */
    @Volatile
    private var groupServer: FramedSocketServer? = null

    private val startGate = SessionGate<WifiP2pManager.Channel> { initialize() }
    private val dnsSdGate = SessionGate<Unit> { registerDnsSd() }

    /**
     * FFI-15: `startDiscovery` used to build a brand-new
     * `WifiP2pDnsSdServiceRequest` and `addServiceRequest` it on every call,
     * with nothing ever calling `removeServiceRequest`/`clearServiceRequests`.
     * `WifiP2pManager` keeps a per-channel list of service requests and
     * re-issues all of them on each `discoverServices` — accumulating
     * duplicates multiplies over-the-air probe traffic and, past the
     * framework's internal limit, makes `addServiceRequest` fail (surfacing
     * to Rust as a plain `TransportError::Io` that kills discovery for good).
     * One request, retained and added once via its own gate, fixes this the
     * same way `dnsSdGate` already avoids re-registering the local service.
     */
    private val serviceRequest: WifiP2pDnsSdServiceRequest by lazy {
        WifiP2pDnsSdServiceRequest.newInstance(DNS_SD_SERVICE_TYPE)
    }
    private val serviceRequestGate = SessionGate<Unit> {
        val channel = startGate.ensureStarted()
        awaitAction { p2pManagerOrThrow().addServiceRequest(channel, serviceRequest, it) }
    }

    /**
     * FFI-5: `startDnsSd()` used to take no arguments — `dnsSdGate`'s lambda
     * captures none, and `registerDnsSd()` registered with `emptyMap()`.
     * `dnsSdGate.ensureStarted()` is lazy, so the real beacon has to be
     * captured here when `startDnsSd()` runs and read back whenever the
     * gate's suspend lambda finally fires.
     */
    @Volatile
    private var pendingOwnBeacon: ByteArray = ByteArray(0)

    private val nextPeerHandle = AtomicLong(1L)
    private val deviceHandles = ConcurrentHashMap<String, Long>()
    private val peerDevices = ConcurrentHashMap<Long, String>() // handle -> P2P device address
    private val discoveredMatches = ConcurrentLinkedQueue<FfiDirectPeerDiscovery>()
    private val pendingTxtBeacons = ConcurrentHashMap<String, ByteArray>()

    private val groupState = GroupState()

    /** Current GO endpoint address, adapter-provided (G-WD-2 — never hard-coded). */
    @Volatile
    private var cachedGoAddr: String? = null

    /**
     * Band hint recorded by `setOperatingBand`, applied at the next group
     * formation (the platform accepts a band only at that point).
     */
    @Volatile
    private var bandHint: FfiOperatingBand = FfiOperatingBand.AUTO

    /**
     * Credentials for a band-pinned autonomous GO. `WifiP2pConfig.Builder`
     * requires both; the platform mandates the `DIRECT-xy` network-name prefix
     * and an 8..63-character passphrase. Generated once per adapter instance so
     * a re-formed group keeps a stable identity within the process lifetime.
     */
    private val groupNetworkName: String =
        "DIRECT-ir-iris%04x".format(SecureRandom().nextInt(0x1_0000))
    private val groupPassphrase: String =
        java.math.BigInteger(1, ByteArray(12).also { SecureRandom().nextBytes(it) })
            .toString(16)
            .padStart(24, '0')

    private val p2pStateReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            when (intent.action) {
                WifiP2pManager.WIFI_P2P_STATE_CHANGED_ACTION -> {
                    val state = intent.getIntExtra(
                        WifiP2pManager.EXTRA_WIFI_STATE,
                        WifiP2pManager.WIFI_P2P_STATE_DISABLED,
                    )
                    availability.setAvailable(state == WifiP2pManager.WIFI_P2P_STATE_ENABLED)
                }
                WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION -> {
                    val group = intent.getParcelableExtra<WifiP2pGroup>(WifiP2pManager.EXTRA_WIFI_P2P_GROUP)
                    val info = intent.getParcelableExtra<WifiP2pInfo>(WifiP2pManager.EXTRA_WIFI_P2P_INFO)
                    group?.let { groupState.update(it) }
                    // Group formation is the only point at which the GO address
                    // and this node's role are known — stand the data path up here.
                    if (info != null && info.groupFormed) {
                        callbackScope.launch { onGroupFormed(info, group) }
                    }
                }
            }
        }
    }

    override suspend fun start() {
        FfiCallTimeout.suspendCall { startGate.ensureStarted() }
    }

    override suspend fun startDnsSd(txtRecord: ByteArray) {
        pendingOwnBeacon = txtRecord
        FfiCallTimeout.suspendCall { dnsSdGate.ensureStarted() }
    }

    override suspend fun stopDnsSd() {
        FfiCallTimeout.suspendCall { dnsSdGate.reset() }
    }

    override suspend fun startDiscovery() {
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted()
            // FFI-15: registers the one retained service request at most
            // once per discovery session instead of accumulating a new one
            // on every call.
            serviceRequestGate.ensureStarted()
            awaitAction { p2pManagerOrThrow().discoverServices(channel, it) }
        }
    }

    override suspend fun stopDiscovery() {
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted()
            awaitAction { p2pManagerOrThrow().stopPeerDiscovery(channel, it) }
            // FFI-15: undo the addServiceRequest from startDiscovery so the
            // next startDiscovery's serviceRequestGate.ensureStarted() call
            // re-adds cleanly rather than silently no-oping on a request the
            // platform no longer has registered.
            if (serviceRequestGate.isStarted()) {
                runCatching {
                    awaitAction { p2pManagerOrThrow().removeServiceRequest(channel, serviceRequest, it) }
                }
                serviceRequestGate.reset()
            }
        }
    }

    override suspend fun matches(): List<FfiDirectPeerDiscovery> {
        return FfiCallTimeout.suspendCall {
            val drained = ArrayList<FfiDirectPeerDiscovery>(discoveredMatches.size)
            while (true) drained.add(discoveredMatches.poll() ?: break)
            drained
        }
    }

    override suspend fun createGroup(config: FfiGroupConfig): FfiGroupInfo {
        return FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted()
                ?: throw TransportFailure("Wi-Fi Direct not initialized")
            val band = if (config.band != FfiOperatingBand.AUTO) config.band else bandHint
            // FFI-9: Rust's band-fallback retry (create_group falling back to
            // OperatingBand::Auto) keys on the substring "band" appearing in
            // the failure message. The only message this path could ever
            // produce was "WifiP2p action failed reason=$reason" — reason is
            // an int (ERROR/P2P_UNSUPPORTED/BUSY/NO_SERVICE_REQUESTS), never
            // the word "band" — so the fallback could never fire on a
            // band-constrained device (regulatory domain, DFS, concurrent
            // STA on an incompatible channel). Tag this specific call site's
            // failure with the band that was actually requested.
            awaitAction(failureContext = "band=$band") { createGroupWithBand(channel, band, it) }
            // AND-RT-103: the group snapshot arrives only via the async
            // CONNECTION_CHANGED broadcast — wait for it, never throw on a
            // momentarily-null snapshot.
            awaitCurrentGroupInfo(channel) ?: degradedGroupInfo()
        }
    }

    override suspend fun joinGroup(go: ULong, config: FfiGroupConfig): FfiGroupInfo {
        return FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted()
                ?: throw TransportFailure("Wi-Fi Direct not initialized")
            val address = peerDevices[go.toLong()]
                ?: throw DeviceNotFound()
            val wifiConfig = WifiP2pConfig().apply { deviceAddress = address }
            awaitAction { p2pManagerOrThrow().connect(channel, wifiConfig, it) }
            awaitCurrentGroupInfo(channel) ?: degradedGroupInfo()
        }
    }

    override suspend fun addClient(client: ULong) {
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted() ?: return@suspendCall
            val address = peerDevices[client.toLong()] ?: return@suspendCall
            val wifiConfig = WifiP2pConfig().apply { deviceAddress = address }
            awaitAction { p2pManagerOrThrow().connect(channel, wifiConfig, it) } // invitation (p2p_invite)
        }
    }

    override suspend fun removeGroup() {
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted() ?: return@suspendCall
            awaitAction { p2pManagerOrThrow().removeGroup(channel, it) }
            closeDataPath()
            groupState.clear()
            cachedGoAddr = null
        }
    }

    /**
     * Tears the data path down: sockets first, then the retained frames for
     * every closed destination (AND-RT-109/110). Leaving the links open would
     * strand accept/read coroutines on a dead radio.
     */
    private fun closeDataPath() {
        groupServer?.close()
        groupServer = null
        links.close()
        groupRegistry.closeAll().forEach {
            verifiedCache.evict(it)
            outbox.drainDestination(it)
            inbox.drainDestination(it)
        }
    }

    override suspend fun groupInfo(): FfiGroupInfo? {
        return FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted() ?: return@suspendCall null
            currentGroupInfo(channel)
        }
    }

    override fun goAddr(): String? = cachedGoAddr

    override suspend fun setOperatingBand(band: FfiOperatingBand) {
        FfiCallTimeout.suspendCall {
            startGate.ensureStarted() ?: return@suspendCall
            // Recorded, not applied here: the platform accepts an operating band
            // only at group formation (see [createGroupWithBand]).
            bandHint = band
        }
    }

    override suspend fun p2pSend(peer: ULong, payload: ByteArray) {
        FfiCallTimeout.suspendCall {
            val handle = peer.toLong()
            val link = links.get(handle)
            if (link != null && link.send(payload)) return@suspendCall
            // FFI-2: this used to always fall through to the outbox and
            // return normally, whether the group was still forming (the
            // legitimate "not yet" case) or the peer's link had already
            // died (a real link-loss event). Rust's entire send-failure
            // taxonomy (is_link_loss_error -> teardown_link vs
            // TransportError::Busy) was dead code against this adapter: a
            // dead group member was retried forever, and Rust recorded a
            // SendReceipt for every dropped frame. groupRegistry.accepts()
            // (opened() on join, closed() on removal) is the same signal
            // already used to prune the inbound drain (RT-109) — use it
            // here too to tell "not a member yet" from "was a member, link
            // is gone".
            if (groupRegistry.accepts(handle)) {
                throw TransportFailure("link closed")
            }
            // No socket yet and never was one (group still forming): hold
            // the frame in the bounded outbox and flush it on connect.
            outbox.enqueue(handle, payload)
        }
    }

    override suspend fun incoming(): List<FfiIncomingWifiDirectData> {
        return FfiCallTimeout.suspendCall {
            val drained = mutableListOf<FfiIncomingWifiDirectData>()
            // Closed-group prune (RT-109): only frames for live group members surface.
            for (peer in groupRegistry.openHandles()) {
                for (frame in inbox.drain(peer)) {
                    drained.add(
                        FfiIncomingWifiDirectData(
                            sender = verifiedCache.verifiedPeerIdFor(peer)?.hexToBytes(),
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
            val channel = startGate.ensureStarted()
            if (channel != null) {
                runCatching { awaitAction { p2pManagerOrThrow().clearLocalServices(channel, it) } }
                // FFI-15: mirror clearLocalServices — drop every accumulated
                // service request too, not just the local advertisement.
                runCatching { awaitAction { p2pManagerOrThrow().clearServiceRequests(channel, it) } }
                runCatching { awaitAction { p2pManagerOrThrow().removeGroup(channel, it) } }
            }
            dnsSdGate.reset()
            serviceRequestGate.reset()
            closeDataPath()
            groupState.clear()
            cachedGoAddr = null
            availability.setAvailable(false)
            runCatching { appContext.unregisterReceiver(p2pStateReceiver) }
        }
    }

    override fun isAvailable(): Boolean =
        p2pManager != null && FfiCallTimeout.syncCall(onTimeout = false) {
            availability.isAvailable()
        }

    // -- platform plumbing -------------------------------------------------

    private val channelListener = object : WifiP2pManager.ChannelListener {
        override fun onChannelDisconnected() {
            // invalidate is suspend (AND-RT-105); lift the platform callback into scope.
            callbackScope.launch { startGate.invalidate() }
            availability.setAvailable(false)
        }
    }

    /**
     * FFI-13: this used to return `null` on a device with no Wi-Fi Direct
     * hardware (`p2pManager == null`) or a failed platform `initialize()`
     * call, and `SessionGate<Channel?>` happily cached that null as its
     * "created" value — `start()` then returned normally, `registerDnsSd()`
     * did `startGate.ensureStarted() ?: return` (a silent success), and the
     * transport was registered, polled forever, and reported a state
     * derived from a side channel (`is_available()`) rather than from the
     * operation that actually failed. `NotSupported` already exists in the
     * FFI error enum and is the correct answer — throwing it here (instead
     * of at some later, unrelated call site) is what lets Rust map straight
     * to a permanent `TransportState::Unavailable` instead of retrying a
     * subsystem that can never come up.
     */
    private fun initialize(): WifiP2pManager.Channel {
        val manager = p2pManager ?: throw NotSupported()
        val channel = runCatching {
            manager.initialize(appContext, Looper.getMainLooper(), channelListener)
        }.getOrNull() ?: throw NotSupported()
        registerStateReceiver()
        availability.setAvailable(true)
        return channel
    }

    private fun registerStateReceiver() {
        val filter = IntentFilter().apply {
            addAction(WifiP2pManager.WIFI_P2P_STATE_CHANGED_ACTION)
            addAction(WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION)
        }
        // API 34+ REQUIRES an export flag; without it registerReceiver throws
        // SecurityException, which runCatching swallowed. The adapter then never
        // saw WIFI_P2P_CONNECTION_CHANGED, so onGroupFormed never ran and the
        // whole TCP-over-GO data path was silently dead on modern devices —
        // while availability still reported true.
        val registered = runCatching {
            ContextCompat.registerReceiver(
                appContext,
                p2pStateReceiver,
                filter,
                ContextCompat.RECEIVER_NOT_EXPORTED,
            )
        }.isSuccess
        if (!registered) availability.setAvailable(false)
    }

    private suspend fun registerDnsSd() {
        val channel = startGate.ensureStarted() ?: return
        // FFI-5: the 22-byte IRIS beacon (WifiDirectTxtRecord::build) is
        // binary, but DNS-SD TXT record values are text — hex-encode it
        // under one well-known key. `dnsSdTxtRecordListener` below decodes
        // the same key back to bytes on the inbound side; without either
        // half, WifiDirectTxtRecord::parse rejects every peer's record and
        // discovery yields zero peers, permanently.
        val serviceInfo = WifiP2pDnsSdServiceInfo.newInstance(
            "iris",
            DNS_SD_SERVICE_TYPE,
            mapOf(BEACON_TXT_KEY to PeerIdCodec.toHex(pendingOwnBeacon)),
        )
        awaitAction { p2pManagerOrThrow().addLocalService(channel, serviceInfo, it) }
        p2pManagerOrThrow().setDnsSdResponseListeners(channel, dnsSdServiceListener, dnsSdTxtRecordListener)
    }

    private val dnsSdServiceListener = WifiP2pManager.DnsSdServiceResponseListener { instanceName, _registrationType, srcDevice ->
        // FFI-1/FFI-18: allocate through the single allocator so this
        // handle is the SAME one join_group/add_client, p2p_send, and the
        // socket data path (attachLink/links/groupRegistry) will all agree
        // on for this device — see peerHandleFor's doc.
        val handle = peerHandleFor(srcDevice.deviceAddress)
        val beacon = pendingTxtBeacons.remove(srcDevice.deviceAddress) ?: ByteArray(0)
        // AND-RT-108: remember the candidate beacon identity (DEC-WA-0007) so the
        // inbound drain can attribute frames to a 64-hex PeerId.
        beaconCandidatePeerIdHex(beacon)?.let { verifiedCache.rememberVerified(handle, it) }
        // FFI-19: `registrationType` is whatever the platform's DNS-SD
        // responder echoes back (observed to carry a trailing ".local."
        // suffix and other framework-version-dependent quirks) — not the
        // fixed value Rust's WIFI_DIRECT_SERVICE_NAME expects to compare
        // against. This listener only fires for responses to the request
        // WE registered under DNS_SD_SERVICE_TYPE (see start_dns_sd /
        // WifiP2pDnsSdServiceRequest.newInstance(DNS_SD_SERVICE_TYPE)
        // above), so report that known-good constant instead of the raw
        // platform echo.
        discoveredMatches.add(
            FfiDirectPeerDiscovery(
                peerHandle = handle.toULong(),
                serviceName = DNS_SD_SERVICE_TYPE,
                txtRecord = beacon,
            ),
        )
    }

    private val dnsSdTxtRecordListener = WifiP2pManager.DnsSdTxtRecordListener { fullDomainName, txtRecordMap, srcDevice ->
        pendingTxtBeacons[srcDevice.deviceAddress] = encodeTxtRecord(txtRecordMap)
    }

    /**
     * FFI-5: decode the hex-encoded beacon `registerDnsSd` publishes under
     * [BEACON_TXT_KEY]. This used to concatenate the whole map as UTF-8
     * "k=v" text — never the 22-byte binary beacon
     * `WifiDirectTxtRecord::parse` requires, so every peer's record failed
     * to parse regardless of what the advertiser side did.
     */
    private fun encodeTxtRecord(map: Map<String, String>): ByteArray =
        map[BEACON_TXT_KEY]?.let { PeerIdCodec.fromHex(it) } ?: ByteArray(0)

    /**
     * Group formation carrying the band hint.
     *
     * The platform has no standalone band setter on [WifiP2pManager]; the
     * operating band is only accepted as a group-formation parameter via
     * `WifiP2pConfig.Builder.setGroupOperatingBand` (API 29+). That builder also
     * requires a network name + passphrase, so a band-pinned group is always a
     * named autonomous GO. Peers still join through `connect()` device-address
     * negotiation (WPS provisioning), so the generated credentials do not have
     * to be shared out-of-band.
     *
     * AUTO — and any band the platform cannot express — falls back to the
     * plain `createGroup` overload and lets the platform choose, matching the
     * core's "band is a hint" contract (`set_operating_band`, wifi_direct.rs).
     */
    private fun createGroupWithBand(
        channel: WifiP2pManager.Channel,
        band: FfiOperatingBand,
        listener: WifiP2pManager.ActionListener,
    ) {
        val bandId = groupOwnerBand(band)
        if (bandId == null || Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) {
            p2pManagerOrThrow().createGroup(channel, listener)
            return
        }
        val p2pConfig = WifiP2pConfig.Builder()
            .setNetworkName(groupNetworkName)
            .setPassphrase(groupPassphrase)
            .setGroupOperatingBand(bandId)
            .build()
        p2pManagerOrThrow().createGroup(channel, p2pConfig, listener)
    }

    /**
     * FFI band -> platform GO band, or `null` when the platform cannot honour
     * it. Wi-Fi Direct exposes no 6 GHz group-owner band, so GHZ6 degrades to
     * the platform default rather than failing group formation outright.
     */
    private fun groupOwnerBand(band: FfiOperatingBand): Int? = when (band) {
        FfiOperatingBand.AUTO -> null
        FfiOperatingBand.GHZ24 -> WifiP2pConfig.GROUP_OWNER_BAND_2GHZ
        FfiOperatingBand.GHZ5 -> WifiP2pConfig.GROUP_OWNER_BAND_5GHZ
        FfiOperatingBand.GHZ6 -> null
    }

    private fun currentGroupInfo(channel: WifiP2pManager.Channel): FfiGroupInfo? {
        val group = groupState.snapshot() ?: return null
        // FFI-10: do NOT overwrite cachedGoAddr with group.owner.deviceAddress
        // (a MAC) here — onGroupFormed already set it to the real, routable
        // info.groupOwnerAddress.hostAddress. This function used to run
        // AFTER onGroupFormed (via createGroup/joinGroup's poll for the
        // settled snapshot) and clobber the correct IP with the MAC on every
        // call.
        return FfiGroupInfo(
            groupId = (group.networkName.hashCode() and 0x7fffffff).toLong().toULong(),
            go = peerHandleFor(group.owner?.deviceAddress).toULong(),
            goAddr = cachedGoAddr,
            clients = group.clientList.mapNotNull { peerHandleFor(it.deviceAddress) }.map { it.toULong() },
        )
    }

    /**
     * AND-RT-103: the WifiP2pGroup snapshot is populated only by the async
     * CONNECTION_CHANGED broadcast. After a successful create/join the adapter
     * polls for the snapshot within a bounded window instead of throwing on a
     * momentarily-null value.
     */
    private suspend fun awaitCurrentGroupInfo(
        channel: WifiP2pManager.Channel,
        waitMs: Long = GROUP_INFO_SETTLE_MS,
    ): FfiGroupInfo? {
        val deadline = SystemClock.elapsedRealtime() + waitMs
        while (SystemClock.elapsedRealtime() < deadline) {
            currentGroupInfo(channel)?.let { return it }
            delay(GROUP_INFO_POLL_MS)
        }
        return null
    }

    /** Null-safe degraded result — never throws on a momentarily-null snapshot. */
    private fun degradedGroupInfo(): FfiGroupInfo = FfiGroupInfo(
        groupId = 0u,
        go = peerHandleFor(cachedGoAddr).toULong(),
        goAddr = cachedGoAddr,
        clients = emptyList(),
    )

    // -- data path ---------------------------------------------------------

    /**
     * Brings the TCP-over-GO data path up once the group exists.
     *
     * The group owner listens on the well-known port; every client dials it.
     * Wi-Fi Direct offers no port negotiation, so the port is a fixed constant
     * both roles agree on.
     */
    private suspend fun onGroupFormed(info: WifiP2pInfo, group: WifiP2pGroup?) {
        // FFI-10: `info.groupOwnerAddress` (an InetAddress, delivered on this
        // SAME broadcast) is the real, routable endpoint — cachedGoAddr used
        // to only ever be set from `WifiP2pGroup.owner.deviceAddress` (a P2P
        // MAC) in `currentGroupInfo`, so `go_addr()`/`FfiGroupInfo.goAddr`
        // handed Rust a MAC where the contract (G-WD-2, "adapter-provided,
        // never hard-coded") promises a dialable address. Today the socket
        // is dialed from `goAddress` directly here, so this was latent — but
        // it silently poisons any future consumer, and made the Rust sim
        // tests (which assert a dotted-quad) unrepresentative of real
        // device behaviour.
        cachedGoAddr = info.groupOwnerAddress?.hostAddress
        if (info.isGroupOwner) {
            ensureGroupServer()
        } else {
            val goAddress = info.groupOwnerAddress ?: return
            connectToGroupOwner(goAddress, peerHandleFor(group?.owner?.deviceAddress))
        }
    }

    /** GO side: accept client links until the group is torn down. */
    private fun ensureGroupServer() {
        if (groupServer != null) return
        // SO_REUSEADDR before bind: after a group teardown the fixed GO port
        // sits in TIME_WAIT, so a plain `ServerSocket(port)` threw, the
        // exception was swallowed, and the data path was silently dead with no
        // error reachable by the core.
        val server = runCatching {
            ServerSocket().apply {
                reuseAddress = true
                bind(java.net.InetSocketAddress(FramedSocketServer.WIFI_DIRECT_GO_PORT))
            }
        }.getOrNull() ?: run {
            availability.setAvailable(false)
            return
        }
        groupServer = FramedSocketServer(server, callbackScope) { socket ->
            // An accepted socket identifies its peer only by IP — Wi-Fi Direct
            // exposes no IP->MAC mapping to the app. The handle is therefore
            // keyed by remote address, and the peer's real identity is resolved
            // from the envelope above the transport (DEC-WA-0007), never from
            // this handle.
            val handle = peerHandleFor(socket.inetAddress?.hostAddress)
            callbackScope.launch { attachLink(handle, socket) }
        }
    }

    /** Client side: dial the group owner, retrying is left to the next group event. */
    private suspend fun connectToGroupOwner(address: InetAddress, handle: Long) {
        if (links.get(handle) != null) return
        val socket = withContext(Dispatchers.IO) {
            runCatching {
                Socket().apply {
                    connect(
                        InetSocketAddress(address, FramedSocketServer.WIFI_DIRECT_GO_PORT),
                        SOCKET_CONNECT_TIMEOUT_MS,
                    )
                }
            }.getOrNull()
        } ?: return
        attachLink(handle, socket)
    }

    /**
     * Registers a live socket for [handle], routes its frames to [inbox], and
     * flushes anything queued while the socket was still coming up.
     */
    private suspend fun attachLink(handle: Long, socket: Socket) {
        val link = FramedSocketLink(
            socket = socket,
            scope = callbackScope,
            onFrame = { frame -> inbox.enqueue(handle, frame) },
            onClosed = {
                links.remove(handle)
                groupServer?.releaseSlot()
            },
        )
        links.put(handle, link)
        groupRegistry.opened(handle)
        for (frame in outbox.drain(handle)) {
            if (!link.send(frame)) {
                // Link died mid-flush: put the frame back and stop draining.
                outbox.enqueue(handle, frame)
                break
            }
        }
    }

    /** The platform manager, or a typed error when the device lacks Wi-Fi Direct. */
    private fun p2pManagerOrThrow(): WifiP2pManager = p2pManager ?: throw NotSupported()

    /**
     * The ONE place a peer handle is ever minted (FFI-1, FFI-18).
     *
     * `dnsSdServiceListener` used to mint its own handle directly from
     * `nextPeerHandle` and write only `peerDevices` (handle -> addr) —
     * never the reverse index `deviceHandles` (addr -> handle) this
     * function reads. Every later call to `peerHandleFor` for the SAME
     * device therefore always missed and allocated a second, different
     * handle: `join_group`/`add_client` (which read `peerDevices`, keyed by
     * the discovery handle) worked, but `p2p_send`/`links`/`groupRegistry`
     * (all keyed by whatever `peerHandleFor` returns) never agreed with it.
     * No Wi-Fi Direct frame could ever be transmitted, in either
     * direction, on real hardware.
     *
     * `ConcurrentHashMap.getOrPut` (the Kotlin extension) is also NOT
     * atomic — it is `get` then `put` — and this function is called
     * concurrently from the broadcast-receiver thread, `callbackScope`
     * coroutines, and the `FramedSocketServer` accept coroutine (FFI-18).
     * `computeIfAbsent` closes that race: two racing callers for the same
     * device now provably get the same handle.
     */
    private fun peerHandleFor(deviceAddress: String?): Long {
        if (deviceAddress == null) return 0L
        val handle = deviceHandles.computeIfAbsent(deviceAddress) { nextPeerHandle.getAndIncrement() }
        peerDevices[handle] = deviceAddress
        return handle
    }

    /**
     * Await a WifiP2pManager async action; the per-call timeout is applied
     * by the caller.
     *
     * [failureContext] (FFI-9), when non-null, is appended to the failure
     * message verbatim — e.g. `"band=$band"` for `createGroup`, so Rust's
     * substring-classified band-fallback retry (which keys on the literal
     * word "band") has something to actually match against. Most callers
     * pass none; the generic reason code is otherwise all Rust ever sees.
     */
    private suspend fun awaitAction(failureContext: String? = null, launch: (WifiP2pManager.ActionListener) -> Unit) {
        suspendCancellableCoroutine { cont ->
            launch(object : WifiP2pManager.ActionListener {
                override fun onSuccess() = cont.resume(Unit)
                override fun onFailure(reason: Int) {
                    // AND-RT-103: typed FFI error — a raw IllegalStateException
                    // becomes UNIFFI_CALL_UNEXPECTED_ERROR and panics the Rust side.
                    val suffix = failureContext?.let { " $it" } ?: ""
                    cont.resumeWithException(TransportFailure("WifiP2p action failed reason=$reason$suffix"))
                }
            })
        }
    }

    private inner class GroupState {
        private var group: WifiP2pGroup? = null

        @Synchronized
        fun update(g: WifiP2pGroup) {
            group = g
            groupRegistry.opened(peerHandleFor(g.owner?.deviceAddress))
            g.clientList.forEach { groupRegistry.opened(peerHandleFor(it.deviceAddress)) }
        }

        @Synchronized
        fun snapshot(): WifiP2pGroup? = group

        @Synchronized
        fun clear() {
            group = null
        }
    }

    private fun String.hexToBytes(): ByteArray {
        if (length % 2 != 0) return ByteArray(0)
        return ByteArray(length / 2) { i -> substring(i * 2, i * 2 + 2).toInt(16).toByte() }
    }
}