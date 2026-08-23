package iriscore.adapter

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.net.wifi.p2p.WifiP2pConfig
import android.net.wifi.p2p.WifiP2pGroup
import android.net.wifi.p2p.WifiP2pManager
import android.net.wifi.p2p.nsd.WifiP2pDnsSdServiceInfo
import android.net.wifi.p2p.nsd.WifiP2pDnsSdServiceRequest
import android.os.Build
import android.os.Looper
import android.os.SystemClock
import iriscode.FfiDirectPeerDiscovery
import iriscode.FfiGroupConfig
import iriscode.FfiGroupInfo
import iriscode.FfiIncomingWifiDirectData
import iriscode.FfiOperatingBand
import iriscode.FfiWifiDirectAdapter
import iriscode.IrisFfiException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.atomic.AtomicLong
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine
import uniffi.iriscode.IrisFfiException

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

        /** Bounded settle window for the async CONNECTION_CHANGED group snapshot. */
        private const val GROUP_INFO_SETTLE_MS = 5_000L

        /** Poll interval while awaiting the group snapshot (AND-RT-103). */
        private const val GROUP_INFO_POLL_MS = 100L
    }

    private val appContext: Context = context.applicationContext
    private val p2pManager: WifiP2pManager =
        appContext.getSystemService(Context.WIFI_P2P_SERVICE) as WifiP2pManager

    /** Lifts non-suspend platform callbacks into suspend [SessionGate] calls. */
    private val callbackScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    private val availability = RadioStateTracker()
    private val groupRegistry = NdpRegistry()
    private val verifiedCache = VerifiedPeerCache()
    private val outbox = RingBufferOutbox<ByteArray>()

    private val startGate = SessionGate<WifiP2pManager.Channel?> { initialize() }
    private val dnsSdGate = SessionGate<Unit> { registerDnsSd() }

    private val nextPeerHandle = AtomicLong(1L)
    private val deviceHandles = ConcurrentHashMap<String, Long>()
    private val peerDevices = ConcurrentHashMap<Long, String>() // handle -> P2P device address
    private val discoveredMatches = CopyOnWriteArrayList<FfiDirectPeerDiscovery>()
    private val pendingTxtBeacons = ConcurrentHashMap<String, ByteArray>()

    private val groupState = GroupState()

    /** Current GO endpoint address, adapter-provided (G-WD-2 — never hard-coded). */
    private var cachedGoAddr: String? = null

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
                    group?.let { groupState.update(it) }
                }
            }
        }
    }

    override suspend fun start() {
        FfiCallTimeout.suspendCall { startGate.ensureStarted() }
    }

    override suspend fun startDnsSd() {
        FfiCallTimeout.suspendCall { dnsSdGate.ensureStarted() }
    }

    override suspend fun stopDnsSd() {
        FfiCallTimeout.suspendCall { dnsSdGate.reset() }
    }

    override suspend fun startDiscovery() {
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted() ?: return@suspendCall
            val request = WifiP2pDnsSdServiceRequest.newInstance(DNS_SD_SERVICE_TYPE)
            awaitAction { p2pManager.addServiceRequest(channel, request, it) }
            awaitAction { p2pManager.discoverServices(channel, it) }
        }
    }

    override suspend fun stopDiscovery() {
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted() ?: return@suspendCall
            awaitAction { p2pManager.stopPeerDiscovery(channel, it) }
        }
    }

    override suspend fun matches(): List<FfiDirectPeerDiscovery> {
        return FfiCallTimeout.suspendCall {
            val drained = discoveredMatches.toList()
            discoveredMatches.clear()
            drained
        }
    }

    override suspend fun createGroup(config: FfiGroupConfig): FfiGroupInfo {
        return FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted()
                ?: throw IrisFfiException.Transport("Wi-Fi Direct not initialized")
            if (config.band != FfiOperatingBand.AUTO) setOperatingBandSuspend(channel, config.band)
            awaitAction { p2pManager.createGroup(channel, it) }
            // AND-RT-103: the group snapshot arrives only via the async
            // CONNECTION_CHANGED broadcast — wait for it, never throw on a
            // momentarily-null snapshot.
            awaitCurrentGroupInfo(channel) ?: degradedGroupInfo()
        }
    }

    override suspend fun joinGroup(go: ULong, config: FfiGroupConfig): FfiGroupInfo {
        return FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted()
                ?: throw IrisFfiException.Transport("Wi-Fi Direct not initialized")
            val address = peerDevices[go.toLong()]
                ?: throw IrisFfiException.DeviceNotFound()
            val wifiConfig = WifiP2pConfig().apply { deviceAddress = address }
            awaitAction { p2pManager.connect(channel, wifiConfig, it) }
            awaitCurrentGroupInfo(channel) ?: degradedGroupInfo()
        }
    }

    override suspend fun addClient(client: ULong) {
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted() ?: return@suspendCall
            val address = peerDevices[client.toLong()] ?: return@suspendCall
            val wifiConfig = WifiP2pConfig().apply { deviceAddress = address }
            awaitAction { p2pManager.connect(channel, wifiConfig, it) } // invitation (p2p_invite)
        }
    }

    override suspend fun removeGroup() {
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted() ?: return@suspendCall
            awaitAction { p2pManager.removeGroup(channel, it) }
            // AND-RT-109/110: closed destinations drop their retained frames.
            groupRegistry.closeAll().forEach {
                verifiedCache.evict(it)
                outbox.drainDestination(it)
            }
            groupState.clear()
            cachedGoAddr = null
        }
    }

    override suspend fun groupInfo(): FfiGroupInfo? {
        return FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted() ?: return@suspendCall
            currentGroupInfo(channel)
        }
    }

    override fun goAddr(): String? = cachedGoAddr

    override suspend fun setOperatingBand(band: FfiOperatingBand) {
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted() ?: return@suspendCall
            setOperatingBandSuspend(channel, band)
        }
    }

    override suspend fun p2pSend(peer: ULong, payload: ByteArray) {
        FfiCallTimeout.suspendCall {
            // TCP-over-GO socket path = AC-6 platform plumbing; ring-buffer here (RT-112).
            outbox.enqueue(peer.toLong(), payload)
        }
    }

    override suspend fun incoming(): List<FfiIncomingWifiDirectData> {
        return FfiCallTimeout.suspendCall {
            val drained = mutableListOf<FfiIncomingWifiDirectData>()
            // Closed-group prune (RT-109): only frames for live group members surface.
            for (peer in groupRegistry.openHandles()) {
                for (frame in outbox.drain(peer)) {
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
                runCatching { awaitAction { p2pManager.clearLocalServices(channel, it) } }
                runCatching { awaitAction { p2pManager.removeGroup(channel, it) } }
            }
            dnsSdGate.reset()
            groupRegistry.closeAll().forEach {
                verifiedCache.evict(it)
                outbox.drainDestination(it)
            }
            groupState.clear()
            cachedGoAddr = null
            availability.setAvailable(false)
            runCatching { appContext.unregisterReceiver(p2pStateReceiver) }
        }
    }

    override fun isAvailable(): Boolean =
        FfiCallTimeout.syncCall(onTimeout = false) { availability.isAvailable() }

    // -- platform plumbing -------------------------------------------------

    private val channelListener = object : WifiP2pManager.ChannelListener {
        override fun onChannelDisconnected() {
            // invalidate is suspend (AND-RT-105); lift the platform callback into scope.
            callbackScope.launch { startGate.invalidate() }
            availability.setAvailable(false)
        }
    }

    private fun initialize(): WifiP2pManager.Channel? {
        val channel = runCatching {
            p2pManager.initialize(appContext, Looper.getMainLooper(), channelListener)
        }.getOrNull() ?: return null
        registerStateReceiver()
        availability.setAvailable(true)
        return channel
    }

    private fun registerStateReceiver() {
        val filter = IntentFilter().apply {
            addAction(WifiP2pManager.WIFI_P2P_STATE_CHANGED_ACTION)
            addAction(WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION)
        }
        runCatching { appContext.registerReceiver(p2pStateReceiver, filter) }
    }

    private suspend fun registerDnsSd() {
        val channel = startGate.ensureStarted() ?: return
        val serviceInfo = WifiP2pDnsSdServiceInfo.newInstance(
            "iris",
            DNS_SD_SERVICE_TYPE,
            emptyMap(),
        )
        awaitAction { p2pManager.addLocalService(channel, serviceInfo, it) }
        p2pManager.setDnsSdResponseListeners(channel, dnsSdServiceListener, dnsSdTxtRecordListener)
    }

    private val dnsSdServiceListener = WifiP2pManager.DnsSdServiceResponseListener { instanceName, registrationType, srcDevice ->
        val handle = nextPeerHandle.getAndIncrement()
        peerDevices[handle] = srcDevice.deviceAddress
        val beacon = pendingTxtBeacons.remove(srcDevice.deviceAddress) ?: ByteArray(0)
        // AND-RT-108: remember the candidate beacon identity (DEC-WA-0007) so the
        // inbound drain can attribute frames to a 64-hex PeerId.
        beaconCandidatePeerIdHex(beacon)?.let { verifiedCache.rememberVerified(handle, it) }
        discoveredMatches.add(
            FfiDirectPeerDiscovery(
                peerHandle = handle.toULong(),
                serviceName = registrationType,
                txtRecord = beacon,
            ),
        )
    }

    private val dnsSdTxtRecordListener = WifiP2pManager.DnsSdTxtRecordListener { fullDomainName, txtRecordMap, srcDevice ->
        pendingTxtBeacons[srcDevice.deviceAddress] = encodeTxtRecord(txtRecordMap)
    }

    /** Best-effort binary reconstruction of the TXT record (device-leg exactness at AC-6). */
    private fun encodeTxtRecord(map: Map<String, String>): ByteArray {
        val sb = StringBuilder()
        for ((k, v) in map) {
            sb.append(k).append('=').append(v).append('\u0000')
        }
        return sb.toString().toByteArray(Charsets.UTF_8)
    }

    private suspend fun setOperatingBandSuspend(channel: WifiP2pManager.Channel, band: FfiOperatingBand) {
        val bandId = when (band) {
            FfiOperatingBand.AUTO -> return
            FfiOperatingBand.GHZ24 -> WifiP2pManager.BAND_24GHZ
            FfiOperatingBand.GHZ5 -> WifiP2pManager.BAND_5GHZ
            FfiOperatingBand.GHZ6 -> WifiP2pManager.BAND_6GHZ
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            awaitAction { p2pManager.setGroupOperatingBand(channel, bandId, it) }
        }
    }

    private fun currentGroupInfo(channel: WifiP2pManager.Channel): FfiGroupInfo? {
        val group = groupState.snapshot() ?: return null
        cachedGoAddr = group.owner?.deviceAddress
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

    private fun peerHandleFor(deviceAddress: String?): Long {
        if (deviceAddress == null) return 0L
        return deviceHandles.getOrPut(deviceAddress) { nextPeerHandle.getAndIncrement() }
    }

    /** Await a WifiP2pManager async action; the per-call timeout is applied by the caller. */
    private suspend fun awaitAction(launch: (WifiP2pManager.ActionListener) -> Unit) {
        suspendCancellableCoroutine { cont ->
            launch(object : WifiP2pManager.ActionListener {
                override fun onSuccess() = cont.resume(Unit)
                override fun onFailure(reason: Int) =
                    // AND-RT-103: typed FFI error — a raw IllegalStateException
                    // becomes UNIFFI_CALL_UNEXPECTED_ERROR and panics the Rust side.
                    cont.resumeWithException(IrisFfiException.Transport("WifiP2p action failed reason=$reason"))
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