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
import iriscode.PermissionDenied
import iriscode.FfiWifiDirectAdapter
import iriscode.Transport
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
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
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
         * Bonjour instance name every IRIS node registers its local service
         * under (`registerDnsSd`'s `WifiP2pDnsSdServiceInfo.newInstance`) —
         * fixed, not per-device, since every peer runs the same code.
         */
        const val DNS_SD_INSTANCE_NAME = "iris"

        /**
         * TXT-record key the 22-byte IRIS beacon (hex-encoded) is published
         * under (FFI-5). DNS-SD TXT values are text; hex is the binary-safe
         * encoding both `registerDnsSd` and `encodeTxtRecord` agree on.
         */
        const val BEACON_TXT_KEY = "b"

        /** Bounded settle window for the async CONNECTION_CHANGED group snapshot. */
        private const val GROUP_INFO_SETTLE_MS = 5_000L

        // HW-11: bounded retry specifically for WifiP2pManager's BUSY (reason=2)
        // — see awaitAction's own doc for why this is correct here.
        private const val BUSY_RETRY_ATTEMPTS = 5
        private const val BUSY_RETRY_DELAY_MS = 500L
        private const val WIFI_P2P_BUSY = 2

        /** Poll interval while awaiting the group snapshot (AND-RT-103). */
        private const val GROUP_INFO_POLL_MS = 100L

        /** Bounded dial timeout for the GO socket; never blocks the callback scope. */
        private const val SOCKET_CONNECT_TIMEOUT_MS = 10_000

        /**
         * FFI-3: bounded read timeout for the GO's blocking handshake-frame
         * read — bigger than a healthy LAN round trip needs, small enough
         * that a client that never sends its handshake can't tie up an
         * accept-loop slot indefinitely.
         */
        private const val HANDSHAKE_TIMEOUT_MS = 5_000

        /** MAC address strings are ~17 bytes; generous ceiling against a hostile length prefix. */
        private const val MAX_HANDSHAKE_BYTES = 64

        /**
         * HV-21: the IRIS beacon frame appended to the handshake. The v2
         * (MAC-carrying) form is 28 bytes; a generous ceiling well above
         * that against a hostile/malformed length prefix, same spirit as
         * [MAX_HANDSHAKE_BYTES].
         */
        private const val MAX_HANDSHAKE_BEACON_BYTES = 512

        /** HV-22: client dead-socket re-dial — retry cadence and total window. */
        private const val REDIAL_RETRY_MS = 2_000L
        private const val REDIAL_WINDOW_MS = 40_000L
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

    private val startGate = SessionGate<WifiP2pManager.Channel>(create = { initialize() })
    // FFI-12: stopDnsSd() used to be nothing but dnsSdGate.reset() — the
    // local DNS-SD advertisement was never actually withdrawn
    // (removeLocalService was never called anywhere in this file), so the
    // device kept advertising IRIS presence after Rust believed
    // stop_advertising had withdrawn it. The gate now carries the
    // WifiP2pDnsSdServiceInfo it registered (previously discarded
    // immediately after addLocalService) so its dispose hook can remove it.
    private val dnsSdGate = SessionGate<WifiP2pDnsSdServiceInfo>(
        create = { registerDnsSd() },
        dispose = { info ->
            val channel = startGate.ensureStarted()
            awaitAction { p2pManagerOrThrow().removeLocalService(channel, info, it) }
        },
    )

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
    // HW-16: `newInstance(serviceType)` (single-arg) is documented — Android's
    // own API reference, confirmed against real live behavior across 2
    // devices this session — to search for the service TYPE only (the PTR
    // record). It does NOT request TXT data. `newInstance(instanceName,
    // serviceType)` is the overload the docs explicitly describe as
    // "Create a service discovery request to get the TXT data from the
    // specified Bonjour service." This is why `dnsSdServiceListener` fired
    // reliably and repeatedly (PTR responses were arriving fine) while
    // `dnsSdTxtRecordListener` never fired even once, live, on either of 2
    // devices, across three separate fix attempts (HW-13/14/15) that all
    // addressed real but secondary issues — none of them could have worked
    // alone, because TXT was never actually being requested at all. Every
    // IRIS node registers under the same fixed instance name
    // (`DNS_SD_INSTANCE_NAME`, `registerDnsSd`'s `WifiP2pDnsSdServiceInfo`),
    // so this is always a valid, known instance to scope the request to.
    private val serviceRequest: WifiP2pDnsSdServiceRequest by lazy {
        WifiP2pDnsSdServiceRequest.newInstance(DNS_SD_INSTANCE_NAME, DNS_SD_SERVICE_TYPE)
    }
    private val serviceRequestGate = SessionGate<Unit>(create = {
        val channel = startGate.ensureStarted()
        awaitAction { p2pManagerOrThrow().addServiceRequest(channel, serviceRequest, it) }
    })

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
    // HW-14: `DnsSdServiceResponseListener`/`DnsSdTxtRecordListener` firing
    // order for the SAME discovered response is not guaranteed — confirmed
    // live: the service listener fired with a real device address while
    // the TXT record listener never fired at all in the same window, so
    // `pendingTxtBeacons` was empty when the service listener looked it up
    // and every peer was silently dropped as an unparseable empty record.
    // Tracks device addresses the service listener has already seen but
    // had no TXT data for yet, so the TXT listener (whichever order it
    // actually arrives in) can finish the job instead of the discovery
    // being lost.
    private val pendingServiceOnly = ConcurrentHashMap<String, Unit>()

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

    /**
     * FFI-3: this node's own P2P device address (MAC), needed for the
     * in-band handshake in [connectToGroupOwner] — a client must tell the
     * GO who it is over the socket, since the GO cannot derive it from the
     * accepted connection alone (see [ensureGroupServer]'s doc).
     */
    @Volatile
    private var myDeviceAddress: String? = null

    /**
     * This node's Wi-Fi Direct device name (e.g. "vivo Y35") from
     * `WIFI_P2P_THIS_DEVICE_CHANGED_ACTION`. Unlike `deviceAddress`, the name
     * is NOT anonymized for a normal app — so it is the one shared, visible
     * coordinate both symmetric IRIS nodes can order themselves by to break
     * the GO-negotiation collision (see [onPeersChanged]).
     */
    private var myDeviceName: String? = null

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
                WifiP2pManager.WIFI_P2P_THIS_DEVICE_CHANGED_ACTION -> {
                    val device = intent.getParcelableExtra<android.net.wifi.p2p.WifiP2pDevice>(
                        WifiP2pManager.EXTRA_WIFI_P2P_DEVICE,
                    )
                    device?.deviceName?.takeIf { it.isNotBlank() }?.let { myDeviceName = it }
                    device?.deviceAddress?.let {
                        myDeviceAddress = it
                        // HV-21: publish for the BLE transport's beacon — see
                        // iriscore.util.LocalWifiDirectAddress's doc.
                        iriscore.util.LocalWifiDirectAddress.address = it
                        iriscore.util.LocalWifiDirectAddress.onChanged?.invoke(it)
                    }
                }
                WifiP2pManager.WIFI_P2P_PEERS_CHANGED_ACTION -> {
                    callbackScope.launch { onPeersChanged() }
                }
                WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION -> {
                    // HV-21: the `EXTRA_WIFI_P2P_INFO` / `EXTRA_WIFI_P2P_GROUP`
                    // intent extras are unreliable on Android 10+ (confirmed
                    // live: a real group formed — `dumpsys wifip2p` showed
                    // `groupFormed: true`, `CONNECTED` — but `info` came back
                    // null in the broadcast, so `onGroupFormed` never ran
                    // and the TCP data path was never stood up). Android's
                    // own docs now say to call `requestConnectionInfo()` /
                    // `requestGroupInfo()` from this receiver instead of
                    // reading the extras. Do that; fall back to the extras
                    // only if the async calls somehow yield nothing.
                    val extraInfo = intent.getParcelableExtra<WifiP2pInfo>(WifiP2pManager.EXTRA_WIFI_P2P_INFO)
                    val extraGroup = intent.getParcelableExtra<WifiP2pGroup>(WifiP2pManager.EXTRA_WIFI_P2P_GROUP)
                    callbackScope.launch {
                        val channel = startGate.ensureStarted()
                        // HW-21: a `false`/absent groupFormed IS the
                        // platform's own disconnect signal — tear the data
                        // path down and clear cached group state
                        // immediately rather than discovering it the hard
                        // way on the next send. Reuses shutdown()'s cleanup.
                        fun teardown() {
                            closeDataPath()
                            groupState.clear()
                            cachedGoAddr = null
                        }
                        if (channel == null) {
                            if (extraInfo != null && extraInfo.groupFormed) {
                                extraGroup?.let { groupState.update(it) }
                                onGroupFormed(extraInfo, extraGroup)
                            } else {
                                teardown()
                            }
                            return@launch
                        }
                        p2pManagerOrThrow().requestConnectionInfo(channel) { info ->
                            val effective = info?.takeIf { it.groupFormed } ?: extraInfo?.takeIf { it.groupFormed }
                            if (effective != null) {
                                p2pManagerOrThrow().requestGroupInfo(channel) { group ->
                                    val g = group ?: extraGroup
                                    g?.let { groupState.update(it) }
                                    callbackScope.launch { onGroupFormed(effective, g) }
                                }
                            } else {
                                teardown()
                            }
                        }
                    }
                }
            }
        }
    }

    override suspend fun start() {
        FfiCallTimeout.suspendCall { startGate.ensureStarted() }
    }

    override suspend fun startDnsSd(txtRecord: ByteArray) {
        rejectAutomaticP2pActivation()
        // HW-13: `ensure_started()` (Rust) calls this with an EMPTY
        // placeholder just to bring the DNS-SD subsystem/response listeners
        // up before any real identity is known (needed for scan-only
        // callers — discover_peers()/connect() with no NodeAdvertisement to
        // build a beacon from); `start_advertising()` then calls this AGAIN
        // moments later with the real 22-byte beacon, on the (Rust-side)
        // assumption the real one "supersedes" the placeholder. It doesn't:
        // `dnsSdGate` — like every `SessionGate` in this file — registers
        // via `create()` (`registerDnsSd()`, one `addLocalService` call)
        // exactly ONCE and caches that forever; whichever of the two calls
        // wins the race to arrive first is what stays registered permanently,
        // with nothing that would ever update it. Confirmed live: the empty
        // placeholder won every single time across 3 real devices, so the
        // DNS-SD TXT record every peer's discovery actually saw advertised
        // an empty/undersized record `WifiDirectTxtRecord::parse` correctly
        // rejects — i.e. this node was never truly discoverable, on every
        // device, despite `start_advertising` reporting success and every
        // scan cycle running cleanly with zero errors. Force a fresh
        // registration whenever the beacon actually changes (covers the
        // placeholder-then-real case here, and also a legitimate future
        // re-announcement after identity/key rotation).
        val beaconChanged = !txtRecord.contentEquals(pendingOwnBeacon)
        val wasStarted = dnsSdGate.isStarted()
        pendingOwnBeacon = txtRecord
        if (beaconChanged && wasStarted) {
            iriscore.util.IrisLog.d("wd.dnssd", "startDnsSd: beacon changed (len=${txtRecord.size}) while already started — forcing re-registration")
            dnsSdGate.reset()
        }
        FfiCallTimeout.suspendCall { dnsSdGate.ensureStarted() }
        iriscore.util.IrisLog.d("wd.dnssd", "startDnsSd DONE len=${txtRecord.size} wasStarted=$wasStarted beaconChanged=$beaconChanged")
        // HW-15: confirmed via research (matches a documented, real-world
        // WifiP2pManager quirk, not speculation) — after addLocalService
        // changes what this node advertises, an ALREADY-RUNNING discovery
        // session (discoverServices()) does not pick up the new TXT data on
        // its own; the framework only (re)transmits the current TXT record
        // in response to a FRESH discoverServices() call issued after the
        // local service change. HW-13 re-registers the local service
        // correctly when the beacon changes, but if `discoverServices()`
        // had already been called earlier (this node's own discovery loop
        // starts independently, on its own ~30s cadence, from
        // `discover_peers()` — not gated on this beacon update at all),
        // that already-in-flight session keeps serving the OLD (possibly
        // still-empty-placeholder) TXT data until its own next unrelated
        // rearm, which live testing showed simply never surfaced a paired
        // TXT record at all. Force an immediate restart here so a beacon
        // change always propagates promptly instead of waiting on an
        // unrelated timer.
        if (beaconChanged && serviceRequestGate.isStarted()) {
            iriscore.util.IrisLog.d("wd.dnssd", "startDnsSd: restarting active discovery session so peers see the new TXT record")
            val channel = startGate.ensureStarted()
            runCatching {
                awaitAction { p2pManagerOrThrow().stopPeerDiscovery(channel, it) }
                // `stopPeerDiscovery()` was observed live to also invalidate
                // the registered service request on at least one real
                // device (reproducing the exact NO_SERVICE_REQUESTS failure
                // FFI-15's own removeServiceRequest+reset already exists to
                // avoid in stopDiscovery() below) — mirror that same
                // defensive re-add here rather than assume the request
                // survives a bare stop/restart.
                serviceRequestGate.reset()
                serviceRequestGate.ensureStarted()
                awaitAction { p2pManagerOrThrow().discoverServices(channel, it) }
            }
        }
    }

    override suspend fun stopDnsSd() {
        FfiCallTimeout.suspendCall { dnsSdGate.reset() }
    }

    override suspend fun startDiscovery() {
        rejectAutomaticP2pActivation()
        // HV-21 (Session 24, stranger-discovery pivot): DNS-SD service
        // discovery (`discoverServices`/`addServiceRequest`/the TXT-record
        // listeners below) was confirmed on real hardware to NEVER resolve
        // — zero PTR/TXT listener callbacks fired across an extended bench
        // session, on two same-vendor phones, regardless of radio state.
        // Plain `discoverPeers()` + `requestPeers()` (below, `onPeersChanged`)
        // finds nearby devices reliably (proven live against these same
        // phones via Android's own Wi-Fi Direct settings screen). This is
        // also what lets Wi-Fi Direct find a TOTAL STRANGER — one never seen
        // over BLE — since it no longer depends on a TXT record (or a BLE
        // beacon) to know a peer exists at all; identity is instead
        // confirmed AFTER connecting, via the handshake extended below
        // (readHandshakeFrame/writeHandshakeFrame now carry the full IRIS
        // beacon, not just a MAC) — "connect first, verify after".
        //
        // HV-21 follow-up: confirmed live on a real device (vivo 2004,
        // Android 12/SDK 31) that `discoverPeers()` can fail with the
        // generic `ERROR` (reason=0) reason on this specific hardware,
        // every single call, while Android's OWN Wi-Fi Direct settings
        // screen (`WifiP2pSettingsActivity`) — same device, same session,
        // zero IRIS code involved — started discovery successfully
        // (`mDiscoveryStarted: true`) moments later. This rules out a
        // platform/hardware limitation (the native path proves discovery
        // genuinely works on this device) and points at a timing issue:
        // IRIS calls this very early in the mesh's lifecycle, right after
        // `startGate.ensureStarted()`'s channel bring-up, and HW-11 already
        // documented that a real device's P2P state machine can take
        // "well over 30 s" to leave its disabled/bringing-up state — that
        // finding only ever saw the platform report `BUSY` during this
        // window, which `awaitAction` already retries; this device instead
        // reports plain `ERROR` for the same underlying "not ready yet"
        // condition. Bounded retry, mirroring HW-11's BUSY constants
        // exactly, scoped to just this call site (not weakening
        // `awaitAction`'s general "every other reason fails immediately"
        // contract everywhere else it's used).
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted()
            var attempt = 0
            while (true) {
                attempt++
                try {
                    awaitAction { p2pManagerOrThrow().discoverPeers(channel, it) }
                    return@suspendCall
                } catch (e: Transport) {
                    if (attempt >= BUSY_RETRY_ATTEMPTS) throw e
                    iriscore.util.IrisLog.w(
                        "wd.stranger",
                        "startDiscovery: discoverPeers failed (attempt $attempt/$BUSY_RETRY_ATTEMPTS) — retrying: ${e.message}",
                    )
                    delay(BUSY_RETRY_DELAY_MS)
                }
            }
        }
    }

    override suspend fun stopDiscovery() {
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted()
            awaitAction { p2pManagerOrThrow().stopPeerDiscovery(channel, it) }
        }
    }

    /**
     * Records the platform addresses of nearby P2P devices without initiating
     * association. `WifiP2pManager.connect()` is deliberately not called from
     * this unsolicited peer-list event: Android turns the same API call into
     * an invitation whenever either side has a P2P group, and OEMs may show a
     * Wi-Fi/Direct coexistence dialog while the STA interface is in use.
     *
     * A normal app cannot accept either system-owned decision on the user's
     * behalf. Keep discovery passive here so background IRIS operation never
     * surfaces an OS prompt; a future explicitly authorized association flow
     * must carry its own user-visible product decision and platform gate.
     */
    private suspend fun onPeersChanged() {
        val channel = startGate.ensureStarted() ?: return
        p2pManagerOrThrow().requestPeers(channel) { peers ->
            for (device in peers.deviceList) {
                val address = device.deviceAddress ?: continue
                peerHandleFor(address)
                iriscore.util.IrisLog.d(
                    "wd.passive",
                    "onPeersChanged: cached nearby device $address (${device.deviceName}); automatic P2P association disabled",
                )
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
                ?: throw Transport("Wi-Fi Direct not initialized")
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
            // FFI-14: this used to fall back to degradedGroupInfo() — a
            // fabricated GroupInfo (groupId=0, empty clients, go derived from
            // a possibly-null cachedGoAddr) presented as a successful result
            // when the CONNECTION_CHANGED broadcast never arrived within the
            // settle window. Rust's connect() then registered a live link and
            // set Connected on the strength of a call that couldn't fail.
            // The settle window not producing a real snapshot means the
            // group genuinely never formed — that's a real, reportable
            // failure, not a degraded-but-ok result.
            awaitCurrentGroupInfo(channel) ?: throw Transport("group not formed")
        }
    }

    override suspend fun joinGroup(go: ULong, config: FfiGroupConfig): FfiGroupInfo {
        return FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted()
                ?: throw Transport("Wi-Fi Direct not initialized")
            val address = peerDevices[go.toLong()]
                ?: throw DeviceNotFound()
            val wifiConfig = WifiP2pConfig().apply { deviceAddress = address }
            awaitAction { p2pManagerOrThrow().connect(channel, wifiConfig, it) }
            // FFI-14: this used to fall back to degradedGroupInfo() — a
            // fabricated GroupInfo (groupId=0, empty clients, go derived from
            // a possibly-null cachedGoAddr) presented as a successful result
            // when the CONNECTION_CHANGED broadcast never arrived within the
            // settle window. Rust's connect() then registered a live link and
            // set Connected on the strength of a call that couldn't fail.
            // The settle window not producing a real snapshot means the
            // group genuinely never formed — that's a real, reportable
            // failure, not a degraded-but-ok result.
            awaitCurrentGroupInfo(channel) ?: throw Transport("group not formed")
        }
    }

    override suspend fun addClient(client: ULong) {
        FfiCallTimeout.suspendCall {
            val channel = startGate.ensureStarted()
            // FFI-14: this used to `return@suspendCall` for an unknown peer —
            // a silent success indistinguishable from a real invitation.
            // Rust's connect() then registered a link and set Connected on
            // the strength of a call that couldn't fail. addClient's own
            // contract (ffi/wifi_direct_adapter.rs: "GO invites a discovered
            // peer") makes an unknown handle a real, reportable error.
            val address = peerDevices[client.toLong()] ?: throw DeviceNotFound()
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
        clientGoDial = null
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
            if (link != null && link.send(payload)) {
                iriscore.util.IrisLog.d("wd.data", "p2pSend handle=$handle len=${payload.size} -> socket ok")
                return@suspendCall
            }
            iriscore.util.IrisLog.w(
                "wd.data",
                "p2pSend handle=$handle len=${payload.size} — no live socket (link=${link != null}, accepts=${groupRegistry.accepts(handle)}); queueing/erroring",
            )
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
                throw Transport("link closed")
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
                    iriscore.util.IrisLog.d("wd.data", "incoming: frame len=${frame.size} from handle=$peer")
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
            // HW-12: only startGate was ever invalidated here. dnsSdGate and
            // serviceRequestGate are each a `create` lambda that registers
            // something (addLocalService / addServiceRequest) against the
            // channel startGate.ensureStarted() hands back — but their own
            // "am I started" state survived a channel disconnect unchanged.
            // The next startDiscovery() call sees the OLD channel is gone,
            // has startGate mint a genuinely NEW one via re-initialize(), but
            // serviceRequestGate still believes itself started and skips
            // re-registering the service request against that new channel —
            // confirmed live: `WifiP2p action failed reason=3`
            // (NO_SERVICE_REQUESTS) calling discoverServices() on a channel
            // that was never actually given a service request. Same root
            // cause would silently break addLocalService's registration
            // (dnsSdGate) the same way — an advertiser that stops responding
            // to discovery after any channel hiccup, with no error at all
            // since discoverServices() itself doesn't require addLocalService
            // to have succeeded. Invalidate all three together so a fresh
            // channel always gets fresh registrations on top of it.
            callbackScope.launch {
                startGate.invalidate()
                dnsSdGate.invalidate()
                serviceRequestGate.invalidate()
            }
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
        val manager = p2pManager ?: run {
            iriscore.util.IrisLog.w("wd.init", "initialize: getSystemService(WIFI_P2P_SERVICE) returned null")
            throw NotSupported()
        }
        // HW verification pass: this used to be `runCatching { }.getOrNull()`,
        // discarding whatever `initialize()` actually threw (permission
        // denial, a null Looper, anything) into an undifferentiated
        // NotSupported — indistinguishable from genuine "no Wi-Fi Direct
        // radio," and confirmed live reproducing identically on all three
        // test devices (Android 12/14/16), which is far more consistent with
        // one shared root cause (most likely a permission gap — Wi-Fi P2P on
        // API 33+ needs NEARBY_WIFI_DEVICES at runtime, not just declared)
        // than three unrelated phones all lacking Wi-Fi Direct hardware.
        val result = runCatching {
            manager.initialize(appContext, Looper.getMainLooper(), channelListener)
        }
        result.exceptionOrNull()?.let {
            iriscore.util.IrisLog.w("wd.init", "initialize: manager.initialize() threw", it)
        }
        val channel = result.getOrNull() ?: run {
            iriscore.util.IrisLog.w("wd.init", "initialize: manager.initialize() returned null channel")
            throw NotSupported()
        }
        registerStateReceiver()
        seedDeviceInfo(channel)
        availability.setAvailable(true)
        return channel
    }

    /**
     * HV-21: `WIFI_P2P_THIS_DEVICE_CHANGED_ACTION` was observed live to never
     * deliver a non-blank `deviceName` on this bench (both phones logged
     * `mine=null`), leaving the collision tie-break in [onPeersChanged] with
     * no coordinate. `requestDeviceInfo` (API 29+) pulls the SAME
     * `WifiP2pDevice` the peer sees — its `deviceName` matches the peer's
     * `device.deviceName` exactly (unlike `Settings.Global.device_name` or
     * `Build.MODEL`, both confirmed to diverge from the advertised P2P name).
     */
    @Suppress("MissingPermission")
    private fun seedDeviceInfo(channel: WifiP2pManager.Channel) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q) return
        runCatching {
            p2pManager?.requestDeviceInfo(channel) { device ->
                device?.deviceName?.takeIf { it.isNotBlank() }?.let { myDeviceName = it }
                device?.deviceAddress
                    ?.takeIf { it.isNotBlank() && it != "02:00:00:00:00:00" }
                    ?.let { myDeviceAddress = it }
            }
        }
    }

    private fun registerStateReceiver() {
        val filter = IntentFilter().apply {
            addAction(WifiP2pManager.WIFI_P2P_STATE_CHANGED_ACTION)
            addAction(WifiP2pManager.WIFI_P2P_CONNECTION_CHANGED_ACTION)
            // FFI-3: source of myDeviceAddress for the client-side handshake.
            addAction(WifiP2pManager.WIFI_P2P_THIS_DEVICE_CHANGED_ACTION)
            // HV-21 (Session 24, stranger-discovery pivot): the plain
            // discoverPeers()/requestPeers() replacement for the DNS-SD path
            // that never resolves on this hardware — see startDiscovery.
            addAction(WifiP2pManager.WIFI_P2P_PEERS_CHANGED_ACTION)
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

    private suspend fun registerDnsSd(): WifiP2pDnsSdServiceInfo {
        val channel = startGate.ensureStarted()
        // FFI-5: the 22-byte IRIS beacon (WifiDirectTxtRecord::build) is
        // binary, but DNS-SD TXT record values are text — hex-encode it
        // under one well-known key. `dnsSdTxtRecordListener` below decodes
        // the same key back to bytes on the inbound side; without either
        // half, WifiDirectTxtRecord::parse rejects every peer's record and
        // discovery yields zero peers, permanently.
        val serviceInfo = WifiP2pDnsSdServiceInfo.newInstance(
            DNS_SD_INSTANCE_NAME,
            DNS_SD_SERVICE_TYPE,
            mapOf(BEACON_TXT_KEY to PeerIdCodec.toHex(pendingOwnBeacon)),
        )
        awaitAction { p2pManagerOrThrow().addLocalService(channel, serviceInfo, it) }
        p2pManagerOrThrow().setDnsSdResponseListeners(channel, dnsSdServiceListener, dnsSdTxtRecordListener)
        // FFI-12: retained (not discarded) so dnsSdGate's dispose hook can
        // removeLocalService with the exact object addLocalService was given.
        return serviceInfo
    }

    private val dnsSdServiceListener = WifiP2pManager.DnsSdServiceResponseListener { instanceName, _registrationType, srcDevice ->
        android.util.Log.d(
            "IrisWifiDirectDiag",
            "dnsSdServiceListener FIRED instanceName=$instanceName registrationType=$_registrationType device=${srcDevice.deviceAddress}",
        )
        // FFI-1/FFI-18: allocate through the single allocator so this
        // handle is the SAME one join_group/add_client, p2p_send, and the
        // socket data path (attachLink/links/groupRegistry) will all agree
        // on for this device — see peerHandleFor's doc.
        val address = srcDevice.deviceAddress
        val beacon = pendingTxtBeacons.remove(address)
        if (beacon == null) {
            // HW-14: TXT record hasn't arrived yet (order not guaranteed) —
            // mark this address so dnsSdTxtRecordListener can finish the
            // job once it does, instead of losing the discovery.
            pendingServiceOnly[address] = Unit
        } else {
            publishDiscoveredMatch(address, beacon)
        }
    }

    private val dnsSdTxtRecordListener = WifiP2pManager.DnsSdTxtRecordListener { fullDomainName, txtRecordMap, srcDevice ->
        android.util.Log.d(
            "IrisWifiDirectDiag",
            "dnsSdTxtRecordListener FIRED fullDomainName=$fullDomainName txtRecordMap=$txtRecordMap device=${srcDevice.deviceAddress}",
        )
        // HW-16 (continued): the TXT callback carries everything needed to
        // publish a discovery on its own (address + beacon) — waiting to
        // pair with the service (PTR) listener is unnecessary and, live,
        // actively harmful: the two-arg `WifiP2pDnsSdServiceRequest`
        // (instanceName+serviceType) this session switched to for TXT data
        // was observed to make `dnsSdServiceListener` stop firing ENTIRELY
        // on at least one real device — the platform appears to answer
        // either the PTR-style query or the TXT-style query per
        // registered request, not always both, at least on some OEM Wi-Fi
        // stacks. Publish directly here; `dnsSdServiceListener` (still kept,
        // still paired via `pendingServiceOnly`/`pendingTxtBeacons` for
        // devices where it DOES fire and TXT arrives first) becomes a
        // secondary, redundant path rather than the only one.
        publishDiscoveredMatch(srcDevice.deviceAddress, encodeTxtRecord(txtRecordMap))
    }

    /**
     * HW-14: the single place a fully-paired (service + TXT record)
     * discovery becomes a Rust-visible `FfiDirectPeerDiscovery`, reachable
     * from either listener depending on which one completes the pair.
     */
    private fun publishDiscoveredMatch(address: String, beacon: ByteArray) {
        val handle = peerHandleFor(address)
        // AND-RT-108: remember the candidate beacon identity (DEC-WA-0007) so the
        // inbound drain can attribute frames to a 64-hex PeerId.
        // HV-21: MUST use the SAME 0xFF-sentinel upper half that
        // `WifiDirectTxtRecord::candidate_peer_id()` (Rust discovery path) and
        // BLE's `DiscoveryBeacon::candidate_peer_id()` (BLE-8) produce — the
        // shared `beaconCandidatePeerIdHex` helper zero-pads (Wi-Fi Aware's
        // convention), so an inbound Wi-Fi Direct frame was attributed to a
        // PeerId the neighbour table / envelope layer had never seen and the
        // message was silently dropped (confirmed live: frame reached the
        // adapter, `msg.delivered` never fired).
        wifiDirectCandidatePeerIdHex(beacon)?.let { verifiedCache.rememberVerified(handle, it) }
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
        // HV-20: the plain no-config `createGroup` overload above is already
        // persistent per Android's own documented behaviour — but this
        // custom-config path (only reached when a band is pinned) builds its
        // own WifiP2pConfig, which defaults to non-persistent unless told
        // otherwise. Without this, a restarted GO on this path would lose
        // its group identity and a previously-joined client couldn't rejoin.
        val p2pConfig = WifiP2pConfig.Builder()
            .setNetworkName(groupNetworkName)
            .setPassphrase(groupPassphrase)
            .setGroupOperatingBand(bandId)
            .enablePersistentMode(true)
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

    // FFI-14: degradedGroupInfo() removed — see the throw Transport
    // call sites above that replaced it.

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
        iriscore.util.IrisLog.d(
            "wd.data",
            "onGroupFormed isGO=${info.isGroupOwner} goAddr=$cachedGoAddr ownerMac=${group?.owner?.deviceAddress} clients=${group?.clientList?.size}",
        )
        if (info.isGroupOwner) {
            ensureGroupServer()
        } else {
            val goAddress = info.groupOwnerAddress ?: run {
                iriscore.util.IrisLog.w("wd.data", "onGroupFormed client: groupOwnerAddress null — cannot dial")
                return
            }
            connectToGroupOwner(goAddress, group?.owner?.deviceAddress, peerHandleFor(group?.owner?.deviceAddress))
        }
    }

    /**
     * GO side: accept client links until the group is torn down.
     *
     * FFI-3: an accepted socket identifies its peer only by IP — Wi-Fi
     * Direct exposes no IP->MAC mapping to the app — but `WifiDirectAdapter
     * ::p2p_send(peer: PeerHandle)` takes the *discovery* handle, which is
     * always MAC-keyed (`peerHandleFor`, the ONE allocator, FFI-1/FFI-18).
     * Keying the accepted link by IP through that same allocator minted a
     * SECOND, disjoint handle for the same physical device — the GO could
     * never resolve a discovery handle to its live link. Fixed with an
     * in-band handshake: the dialing client sends its own P2P device
     * address as the very first frame on the socket (see
     * [connectToGroupOwner]); the GO reads it here before building the
     * normal framed link, and resolves the SAME MAC-keyed handle discovery
     * already uses.
     */
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
            iriscore.util.IrisLog.w("wd.data", "ensureGroupServer: bind to GO port ${FramedSocketServer.WIFI_DIRECT_GO_PORT} FAILED")
            availability.setAvailable(false)
            return
        }
        iriscore.util.IrisLog.d("wd.data", "ensureGroupServer: listening on GO port ${FramedSocketServer.WIFI_DIRECT_GO_PORT}")
        groupServer = FramedSocketServer(server, callbackScope) { socket ->
            // Bounded blocking read/write for the handshake — a slow or
            // malicious client must not be able to stall the accept loop
            // (which this callback runs on) indefinitely. A timeout or
            // malformed handshake throws, which FramedSocketServer's own
            // try/catch around this callback already handles (releases the
            // concurrent-link slot, closes the socket) — so a peer that
            // never completes the handshake is simply dropped, no link
            // ever attached, nothing surfaced to the user.
            socket.soTimeout = HANDSHAKE_TIMEOUT_MS
            val (clientMac, clientBeacon) = readHandshakeFrame(socket)
            // Non-suspend context (this callback runs synchronously on the
            // accept loop) — `myDeviceAddress` is a plain field, already
            // populated by `WIFI_P2P_THIS_DEVICE_CHANGED_ACTION` well before
            // any group forms; unlike the client side there is no need to
            // poll for it here.
            writeHandshakeFrame(socket, myDeviceAddress ?: "", pendingOwnBeacon)
            socket.soTimeout = 0
            // HV-21 (stranger discovery): report this handle to Rust's
            // discover_peers() exactly like a DNS-SD match used to —
            // `publishDiscoveredMatch` already mints/caches the handle and
            // remembers the candidate identity (AND-RT-108); just sourced
            // from a verified POST-connect handshake instead of a
            // pre-connect TXT record. `WifiDirectTxtRecord::parse` (Rust) is
            // what actually validates `clientBeacon` — a peer that sent
            // garbage here simply never yields a `PeerInfo`; the link stays
            // open but inert (harmless: nothing ever addresses messages to
            // an unresolved handle).
            iriscore.util.IrisLog.d("wd.data", "GO accepted client: mac=$clientMac beaconLen=${clientBeacon.size} — handshake ok")
            publishDiscoveredMatch(clientMac, clientBeacon)
            val handle = peerHandleFor(clientMac)
            callbackScope.launch { attachLink(handle, socket) }
        }
    }

    /**
     * Client side: dial the group owner, retrying is left to the next group
     * event. HV-21: the handshake is now bidirectional and carries each
     * side's full IRIS beacon (not just a MAC) — "connect first, verify
     * after" for a peer discovered via plain `discoverPeers()` with no
     * prior identity at all.
     */
    /**
     * HV-22: the last GO the client dialed, so [attachLink]'s `onClosed` can
     * re-dial when the socket drops but the OS group is still up (e.g. the GO's
     * app restarted — the group persists as an autonomous GO, the client stays
     * `groupFormed: true`, so no `CONNECTION_CHANGED` fires to re-trigger
     * `onGroupFormed`, and without this the client would sit on a dead socket
     * until a send failed and never recover). Cleared on group teardown.
     */
    @Volatile private var clientGoDial: Triple<InetAddress, String?, Long>? = null

    private suspend fun connectToGroupOwner(address: InetAddress, goMac: String?, handle: Long) {
        if (links.get(handle) != null) return
        clientGoDial = Triple(address, goMac, handle)
        // FFI-3: the GO cannot identify an accepted socket's peer by MAC on
        // its own — tell it who we are as the first thing on the wire, in
        // the same MAC-string form discovery already keys peerHandleFor by.
        val myMac = awaitMyDeviceAddress() ?: run {
            iriscore.util.IrisLog.w("wd.data", "connectToGroupOwner: own device address never arrived — cannot handshake")
            return
        }
        iriscore.util.IrisLog.d("wd.data", "connectToGroupOwner: dialing GO $address:${FramedSocketServer.WIFI_DIRECT_GO_PORT} as $myMac")
        val goBeacon = withContext(Dispatchers.IO) {
            runCatching {
                val socket = Socket().apply {
                    connect(
                        InetSocketAddress(address, FramedSocketServer.WIFI_DIRECT_GO_PORT),
                        SOCKET_CONNECT_TIMEOUT_MS,
                    )
                    soTimeout = HANDSHAKE_TIMEOUT_MS
                    writeHandshakeFrame(this, myMac, pendingOwnBeacon)
                }
                val (_, beacon) = readHandshakeFrame(socket)
                socket.soTimeout = 0
                socket to beacon
            }.onFailure {
                iriscore.util.IrisLog.w("wd.data", "connectToGroupOwner: dial/handshake to $address FAILED", it)
            }.getOrNull()
        } ?: return
        val (socket, beacon) = goBeacon
        iriscore.util.IrisLog.d("wd.data", "connectToGroupOwner: handshake ok with GO $address, goBeaconLen=${beacon.size}")
        // HV-21: the GO's own address, not this node's — peerHandleFor(myMac)
        // would mint/resolve the WRONG handle (ours, not theirs). `handle`
        // (the param) is already the GO's discovery handle; publish under
        // its address so publishDiscoveredMatch's own peerHandleFor lookup
        // resolves back to the SAME handle rather than minting a new one.
        val goAddress = peerDevices[handle] ?: goMac
        if (goAddress != null) {
            publishDiscoveredMatch(goAddress, beacon)
        } else {
            iriscore.util.IrisLog.w("wd.data", "connectToGroupOwner: no GO MAC to publish discovery match under — Rust send() will not resolve this link")
        }
        attachLink(handle, socket)
        iriscore.util.IrisLog.d("wd.data", "connectToGroupOwner: link attached handle=$handle")
    }

    /**
     * HV-22: re-dial the GO after a client socket drop while the OS group is
     * still up (the GO's app restarted, say). Bounded retry — the GO's
     * `ServerSocket` may take a few seconds to re-bind after its process comes
     * back. Stops as soon as a link attaches, the dial record is cleared
     * (deliberate teardown), or the group actually goes away.
     */
    private suspend fun redialGroupOwner(address: InetAddress, goMac: String?, handle: Long) {
        val deadline = SystemClock.elapsedRealtime() + REDIAL_WINDOW_MS
        while (SystemClock.elapsedRealtime() < deadline) {
            if (links.get(handle) != null) return
            if (clientGoDial?.third != handle) return
            val group = groupState.snapshot()
            if (group != null && group.isGroupOwner) return
            iriscore.util.IrisLog.d("wd.data", "redialGroupOwner: retrying dial to GO $address")
            connectToGroupOwner(address, goMac, handle)
            if (links.get(handle) != null) return
            delay(REDIAL_RETRY_MS)
        }
        iriscore.util.IrisLog.w("wd.data", "redialGroupOwner: gave up after ${REDIAL_WINDOW_MS}ms")
    }

    /**
     * Polls briefly for [myDeviceAddress] — populated by the
     * `WIFI_P2P_THIS_DEVICE_CHANGED_ACTION` broadcast, which normally fires
     * well before any group forms, but isn't guaranteed to have landed yet
     * the very first time a group is joined.
     */
    private suspend fun awaitMyDeviceAddress(): String? {
        val deadline = SystemClock.elapsedRealtime() + GROUP_INFO_SETTLE_MS
        while (SystemClock.elapsedRealtime() < deadline) {
            myDeviceAddress?.let { return it }
            delay(GROUP_INFO_POLL_MS)
        }
        return myDeviceAddress
    }

    /**
     * FFI-3 handshake wire format, extended for HV-21 (stranger discovery):
     * TWO length-prefixed frames back to back — the MAC address string
     * (ASCII, as before) followed by the sender's IRIS beacon
     * (`pendingOwnBeacon`, the same bytes DNS-SD used to carry as a TXT
     * record — reused verbatim, no new encoding). Same 4-byte
     * big-endian-length framing as [FrameCodec], written/read directly on
     * the socket's raw streams (never `.buffered()`) — a buffered wrapper
     * here would read ahead past the handshake bytes into the first real
     * frame, which [FramedSocketLink]'s own separate buffered reader (built
     * afterward) would then never see. This is now bidirectional: both
     * sides write, then both sides read, before either builds the framed
     * link — "connect first, verify after" needs each side to actually
     * learn the other's identity, not just the GO learning the client's MAC.
     * A peer whose beacon frame is missing/malformed never yields a valid
     * `WifiDirectTxtRecord::parse` on the Rust side (that's where real
     * validation happens, not here) — the link is simply never resolved to
     * a `PeerId` and nothing is ever addressed to it.
     */
    private fun writeHandshakeFrame(socket: Socket, mac: String, beacon: ByteArray) {
        val out = java.io.DataOutputStream(socket.getOutputStream())
        val macBytes = mac.toByteArray(Charsets.US_ASCII)
        out.writeInt(macBytes.size)
        out.write(macBytes)
        out.writeInt(beacon.size)
        out.write(beacon)
        out.flush()
    }

    /** @return (peerMac, peerBeacon). */
    private fun readHandshakeFrame(socket: Socket): Pair<String, ByteArray> {
        val input = java.io.DataInputStream(socket.getInputStream())
        val macLen = input.readInt()
        if (macLen <= 0 || macLen > MAX_HANDSHAKE_BYTES) {
            throw java.io.IOException("handshake mac length $macLen out of range")
        }
        val macBytes = ByteArray(macLen)
        input.readFully(macBytes)
        val beaconLen = input.readInt()
        if (beaconLen < 0 || beaconLen > MAX_HANDSHAKE_BEACON_BYTES) {
            throw java.io.IOException("handshake beacon length $beaconLen out of range")
        }
        val beaconBytes = ByteArray(beaconLen)
        input.readFully(beaconBytes)
        return String(macBytes, Charsets.US_ASCII) to beaconBytes
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
                iriscore.util.IrisLog.d("wd.data", "link closed handle=$handle")
                // HV-22: client-side dead-socket recovery — see clientGoDial.
                val dial = clientGoDial
                if (dial != null && dial.third == handle) {
                    callbackScope.launch { redialGroupOwner(dial.first, dial.second, dial.third) }
                }
            },
        )
        links.put(handle, link)
        groupRegistry.opened(handle)
        iriscore.util.IrisLog.d("wd.data", "attachLink: socket live for handle=$handle")
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
     * Samsung's Sharing mode can present a system dialog for DNS-SD service
     * registration or peer discovery alone, even when no association is
     * attempted. Disable every background P2P activation route until IRIS has
     * a platform-safe, explicitly authorized Wi-Fi Direct design.
     */
    private fun rejectAutomaticP2pActivation(): Nothing = throw NotSupported()

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
    // HW-11: WifiP2pManager, like BluetoothGatt (HW-6/HW-7), tolerates only
    // one outstanding action per channel — confirmed live on a real S24
    // Ultra: `discoverServices()` (fired by the discovery poll loop) and
    // `addLocalService()` (fired by `start_advertising`'s `registerDnsSd()`)
    // both go through this SAME channel via separate, independently-gated
    // suspend functions (`serviceRequestGate`, `dnsSdGate`) with nothing
    // stopping them from being in flight at once — and when they were,
    // Android returned `WifiP2p action failed reason=2` (BUSY) rather than
    // queuing the second request. Every `awaitAction` call site now
    // serializes on this single mutex, so at most one WifiP2pManager action
    // is ever in flight for this adapter, regardless of which caller issued
    // it — the same fix shape as HW-7's GATT write completion wait, applied
    // to the analogous one-op-at-a-time constraint on this platform API.
    private val actionMutex = Mutex()

    // HW-11: BUSY (reason=2) survived the mutex above on a real Samsung S24
    // Ultra even with nothing else of OURS in flight — `dumpsys wifip2p`
    // showed the platform's own P2P state machine taking well over the
    // adapter's fixed 30s per-call timeout to leave P2pDisabledState on this
    // device, meaning our very first request can legitimately still be
    // "busy" from the platform's own slow bring-up, not from anything this
    // adapter did wrong. BUSY is Android's documented signal for exactly
    // this kind of transient local contention (the same category HW-6/7
    // already established for BluetoothGatt's analogous case) — retrying
    // the specific failed call after a short backoff is correct here in a
    // way blind retry is not: every OTHER failure reason (P2P_UNSUPPORTED,
    // ERROR, a permission denial) still fails immediately, unchanged.
    private suspend fun awaitAction(failureContext: String? = null, launch: (WifiP2pManager.ActionListener) -> Unit) {
        // HV-23: the BUSY backoff `delay` used to run INSIDE `actionMutex.withLock`,
        // so a caller stuck retrying a BUSY platform action (HW-11: up to
        // BUSY_RETRY_ATTEMPTS * BUSY_RETRY_DELAY_MS = 2.5s, and every other
        // caller queued behind that one during platform bring-up) held the
        // mutex for the ENTIRE backoff window, serializing every other
        // Wi-Fi Direct operation (including start/startDnsSd/startDiscovery)
        // behind it. The mutex only needs to protect "one platform action in
        // flight at a time" (HW-7's constraint) — it does not need to also
        // cover the sleep between retries of the SAME action. Acquiring the
        // lock fresh per attempt still guarantees at most one in-flight
        // action, but lets other queued callers interleave during the sleep.
        var attempt = 0
        while (true) {
            attempt++
            try {
                actionMutex.withLock { awaitActionOnce(failureContext, launch) }
                return
            } catch (e: Transport) {
                val busy = e.message?.contains("reason=$WIFI_P2P_BUSY") == true
                if (!busy || attempt >= BUSY_RETRY_ATTEMPTS) throw e
                delay(BUSY_RETRY_DELAY_MS)
            }
        }
    }

    private suspend fun awaitActionOnce(failureContext: String?, launch: (WifiP2pManager.ActionListener) -> Unit) {
        suspendCancellableCoroutine { cont ->
            try {
                launch(object : WifiP2pManager.ActionListener {
                    override fun onSuccess() = cont.resume(Unit)
                    override fun onFailure(reason: Int) {
                        // AND-RT-103: typed FFI error — a raw IllegalStateException
                        // becomes UNIFFI_CALL_UNEXPECTED_ERROR and panics the Rust side.
                        val suffix = failureContext?.let { " $it" } ?: ""
                        cont.resumeWithException(Transport("WifiP2p action failed reason=$reason$suffix"))
                    }
                })
            } catch (e: SecurityException) {
                // FFI-7: discoverServices/addServiceRequest/addLocalService/
                // connect/createGroup all throw SecurityException
                // synchronously, before any listener fires, when
                // NEARBY_WIFI_DEVICES (API 33+) / ACCESS_FINE_LOCATION
                // (API <=32) isn't granted. Undeclared, this became
                // UNIFFI_CALL_UNEXPECTED_ERROR and aborted the Rust call —
                // the FFI contract already declares PermissionDenied as the
                // distinguishable outcome; construct it here so Rust can
                // finally tell "permission denied" from "radio busy".
                cont.resumeWithException(PermissionDenied())
            }
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
