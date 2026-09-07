package iriscore.adapter

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.util.Log
import iriscode.IrisEngine
import java.util.concurrent.CopyOnWriteArraySet
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/**
 * Tier-5 LAN discovery and advertising adapter.
 *
 * Registers `_iris._tcp` via [NsdManager] so IRIS nodes on the same
 * infrastructure Wi-Fi can discover one another without a relay:
 *
 * - **Advertising:** starts the Rust LAN listener (plain-TCP, ephemeral port)
 *   and registers a `_iris._tcp` service with that port.  The TXT record
 *   carries `v=1` (protocol version) and `id=<nodeIdHex>` (32-byte node id
 *   as 64-hex) so the remote side can skip connecting to itself.
 * - **Discovery:** discovers `_iris._tcp` services from peers, resolves each
 *   to an IP:port string, and calls [IrisEngine.setInternetRelayEndpoints]
 *   so the Rust transport can establish a plain-TCP connection to them.
 *   (HV-43: once the full LAN-direct path lands in the transport the call
 *   site will switch to a dedicated LAN-peer FFI instead of relay endpoints.)
 *
 * All NsdManager callbacks are posted to the main thread by the platform;
 * this class serialises its own mutable state with [AtomicBoolean] /
 * [CopyOnWriteArraySet] and never blocks a callback.
 *
 * Lifecycle: [start] is idempotent; [stop] cancels both registration and
 * discovery, unregisters the service, and leaves the object ready for a
 * future [start] (e.g. after a network change).
 */
class AndroidNsdAdapter(
    context: Context,
    private val engine: IrisEngine,
) {
    private val nsd = context.getSystemService(NsdManager::class.java)
    private val nodeIdHex: String = engine.nodeId().joinToString("") { "%02x".format(it) }
    private val started = AtomicBoolean(false)

    /** IP:port strings of currently live discovered peers. */
    private val lanPeers = CopyOnWriteArraySet<String>()

    /** Port the Rust LAN listener is bound on; 0 until [start]. */
    private val lanPort = AtomicInteger(0)

    // ---- NsdManager listeners -----------------------------------------------

    private val registrationListener = object : NsdManager.RegistrationListener {
        override fun onServiceRegistered(info: NsdServiceInfo) {
            Log.i(TAG, "NSD registered: ${info.serviceName}")
        }
        override fun onRegistrationFailed(info: NsdServiceInfo, code: Int) {
            Log.w(TAG, "NSD registration failed: error $code for ${info.serviceName}")
        }
        override fun onServiceUnregistered(info: NsdServiceInfo) {
            Log.i(TAG, "NSD unregistered: ${info.serviceName}")
        }
        override fun onUnregistrationFailed(info: NsdServiceInfo, code: Int) {
            Log.w(TAG, "NSD unregistration failed: error $code")
        }
    }

    private val discoveryListener = object : NsdManager.DiscoveryListener {
        override fun onDiscoveryStarted(serviceType: String) {
            Log.i(TAG, "NSD discovery started: $serviceType")
        }
        override fun onDiscoveryStopped(serviceType: String) {
            Log.i(TAG, "NSD discovery stopped: $serviceType")
        }
        override fun onStartDiscoveryFailed(serviceType: String, code: Int) {
            Log.w(TAG, "NSD discovery start failed: error $code")
        }
        override fun onStopDiscoveryFailed(serviceType: String, code: Int) {
            Log.w(TAG, "NSD discovery stop failed: error $code")
        }

        override fun onServiceFound(info: NsdServiceInfo) {
            if (info.serviceType != SERVICE_TYPE) return
            // Skip own advertisement (same node id in TXT).
            val txt = info.attributes
            val foundId = txt["id"]?.toString(Charsets.UTF_8)
            if (foundId == nodeIdHex) return
            nsd.resolveService(info, makeResolveListener())
        }
        override fun onServiceLost(info: NsdServiceInfo) {
            Log.i(TAG, "NSD service lost: ${info.serviceName}")
            // Best-effort removal; the set may not contain the exact string
            // without the resolved IP, so we rebuild from remaining discovered.
            pushLanPeers()
        }
    }

    // -------------------------------------------------------------------------

    /**
     * Start advertising and discovery. Safe to call multiple times (idempotent).
     *
     * @throws IllegalStateException if the Rust LAN listener could not bind.
     */
    fun start() {
        if (!started.compareAndSet(false, true)) return

        val port = try {
            engine.startInternetLanListener().also { lanPort.set(it.toInt()) }
        } catch (e: Exception) {
            Log.e(TAG, "Failed to start LAN listener: ${e.message}")
            started.set(false)
            return
        }

        // Advertise own service.
        val info = NsdServiceInfo().apply {
            serviceName = "iris-${nodeIdHex.take(12)}"
            serviceType = SERVICE_TYPE
            this.port = port.toInt()
            setAttribute("v", "1")
            setAttribute("id", nodeIdHex)
        }
        nsd.registerService(info, NsdManager.PROTOCOL_DNS_SD, registrationListener)

        // Discover peers.
        nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, discoveryListener)
    }

    /** Stop advertising and discovery. Idempotent. */
    fun stop() {
        if (!started.compareAndSet(true, false)) return
        runCatching { nsd.unregisterService(registrationListener) }
        runCatching { nsd.stopServiceDiscovery(discoveryListener) }
        lanPeers.clear()
        lanPort.set(0)
    }

    // -------------------------------------------------------------------------

    private fun makeResolveListener(): NsdManager.ResolveListener =
        object : NsdManager.ResolveListener {
            override fun onResolveFailed(info: NsdServiceInfo, code: Int) {
                Log.w(TAG, "NSD resolve failed: error $code for ${info.serviceName}")
            }

            override fun onServiceResolved(info: NsdServiceInfo) {
                val host = info.host?.hostAddress ?: return
                val port = info.port
                if (port <= 0) return
                val addr = "$host:$port"

                // Extract peer identity from TXT record.
                val peerIdHex = info.attributes["id"]?.toString(Charsets.UTF_8)
                if (peerIdHex == null || peerIdHex.length != 64) {
                    Log.w(TAG, "NSD resolved peer missing 64-char id TXT: $addr")
                    return
                }

                Log.i(TAG, "NSD resolved LAN peer: $addr peer=$peerIdHex")
                lanPeers.add(addr)

                // Register as a plain-TCP LAN peer; the engine routes sends to
                // this peer directly without going through the relay.
                runCatching {
                    engine.addInternetLanPeer(peerIdHex, addr)
                }.onFailure { e ->
                    Log.w(TAG, "Failed to register LAN peer with engine: ${e.message}")
                }
            }
        }

    /**
     * No-op: LAN peers are now registered directly in [onServiceResolved] via
     * [engine.addInternetLanPeer].  Kept for [onServiceLost] compatibility.
     */
    private fun pushLanPeers() {
        // Individual peer removal on service loss is not yet implemented;
        // the engine will fail to send and fall back to the relay on next
        // connection attempt.
    }

    companion object {
        private const val TAG = "IrisNsd"
        const val SERVICE_TYPE = "_iris._tcp."
    }
}
