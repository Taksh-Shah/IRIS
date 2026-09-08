package iriscore.adapter

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import android.os.Handler
import android.os.Looper
import android.util.Log
import iriscode.IrisEngine
import java.net.Inet6Address
import java.net.InetAddress
import java.util.ArrayDeque
import java.util.concurrent.Executors

/** Worker-thread-confined ownership for the native LAN listener lease. */
internal class LanListenerLease {
    private var ownerGeneration: Int? = null

    fun acquired(generation: Int) {
        ownerGeneration = generation
    }

    fun releaseIfOwned(generation: Int): Boolean {
        if (ownerGeneration != generation) return false
        ownerGeneration = null
        return true
    }

    fun clear() {
        ownerGeneration = null
    }

    fun owns(generation: Int): Boolean = ownerGeneration == generation
}

/** Serialized, non-blocking Android NSD bridge for authenticated IRIS LAN links. */
class AndroidNsdAdapter(context: Context, private val engine: IrisEngine) {
    private data class ResolveRequest(val info: NsdServiceInfo, val generation: Int)
    private data class LanHint(val peerId: String, val address: String)

    private val nsd = context.getSystemService(NsdManager::class.java)
    private val wifi = context.applicationContext.getSystemService(WifiManager::class.java)
    private val handler = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "iris-nsd-ffi").apply { isDaemon = true }
    }
    // Accessed only on `worker`; makes stale cleanup safe across stop/start.
    private val listenerLease = LanListenerLease()
    private val nodeIdHex = engine.nodeId().joinToString("") { "%02x".format(it) }
    private val multicastLock = wifi.createMulticastLock("iris-nsd").apply { setReferenceCounted(false) }

    // All fields below are main-looper confined.
    private var desired = false
    private var started = false
    private var generation = 0
    private var retryAttempt = 0
    private var nextResolveToken = 0
    private var activeResolveToken: Int? = null
    private val resolveQueue = ArrayDeque<ResolveRequest>()
    private val lostServices = mutableSetOf<String>()
    private val hintsByService = mutableMapOf<String, LanHint>()
    private val registeredServices = mutableMapOf<String, String>()
    private var registrationListener: NsdManager.RegistrationListener? = null
    private var discoveryListener: NsdManager.DiscoveryListener? = null

    fun start() = onMain { desired = true; startAttempt() }

    fun stop() = onMain {
        desired = false
        generation += 1
        teardownAttempt()
    }

    /** Retry cached discovery hints after out-of-band trust confirmation. */
    fun onPeerTrusted(peerIdHex: String) = onMain {
        hintsByService.filterValues { it.peerId == peerIdHex.lowercase() }
            .forEach { (service, hint) -> submitHint(service, hint, generation) }
    }

    private fun startAttempt() {
        if (!desired || started) return
        started = true
        generation += 1
        val attemptGeneration = generation
        runCatching { multicastLock.acquire() }
            .onFailure { Log.w(TAG, "Unable to acquire multicast lock: ${it.message}") }
        worker.execute {
            val result = runCatching { engine.startInternetLanListener().toInt() }
            if (result.isSuccess) listenerLease.acquired(attemptGeneration)
            handler.post {
                if (!isCurrent(attemptGeneration)) {
                    if (result.isSuccess) worker.execute {
                        if (listenerLease.releaseIfOwned(attemptGeneration)) {
                            runCatching { engine.stopInternetLanListener() }
                        }
                    }
                    return@post
                }
                result.onSuccess { startAndroidNsd(attemptGeneration, it) }
                    .onFailure { failAttempt(attemptGeneration, "LAN listener", it.message) }
            }
        }
    }

    private fun startAndroidNsd(attemptGeneration: Int, port: Int) {
        val registration = makeRegistrationListener(attemptGeneration)
        val discovery = makeDiscoveryListener(attemptGeneration)
        registrationListener = registration
        discoveryListener = discovery
        val info = NsdServiceInfo().apply {
            serviceName = "iris-${nodeIdHex.take(12)}"
            serviceType = SERVICE_TYPE
            this.port = port
            setAttribute("v", "1")
            setAttribute("id", nodeIdHex)
        }
        runCatching {
            nsd.registerService(info, NsdManager.PROTOCOL_DNS_SD, registration)
            nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, discovery)
        }.onFailure { failAttempt(attemptGeneration, "NSD start", it.message) }
    }

    private fun makeRegistrationListener(attemptGeneration: Int) =
        object : NsdManager.RegistrationListener {
            override fun onServiceRegistered(info: NsdServiceInfo) = onMain {
                if (isCurrent(attemptGeneration)) Log.i(TAG, "NSD registered: ${info.serviceName}")
            }
            override fun onRegistrationFailed(info: NsdServiceInfo, code: Int) = onMain {
                failAttempt(attemptGeneration, "registration", "error $code for ${info.serviceName}")
            }
            override fun onServiceUnregistered(info: NsdServiceInfo) {
                Log.i(TAG, "NSD unregistered: ${info.serviceName}")
            }
            override fun onUnregistrationFailed(info: NsdServiceInfo, code: Int) {
                Log.w(TAG, "NSD unregistration failed: error $code")
            }
        }

    private fun makeDiscoveryListener(attemptGeneration: Int) =
        object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String) = onMain {
                if (isCurrent(attemptGeneration)) retryAttempt = 0
            }
            override fun onDiscoveryStopped(serviceType: String) {
                Log.i(TAG, "NSD discovery stopped: $serviceType")
            }
            override fun onStartDiscoveryFailed(serviceType: String, code: Int) = onMain {
                failAttempt(attemptGeneration, "discovery start", "error $code")
            }
            override fun onStopDiscoveryFailed(serviceType: String, code: Int) {
                Log.w(TAG, "NSD discovery stop failed: error $code")
            }
            override fun onServiceFound(info: NsdServiceInfo) = onMain {
                if (!isCurrent(attemptGeneration) || info.serviceType != SERVICE_TYPE) return@onMain
                if (info.attributes["id"]?.toString(Charsets.UTF_8) == nodeIdHex) return@onMain
                lostServices.remove(info.serviceName)
                resolveQueue.add(ResolveRequest(info, attemptGeneration))
                resolveNext()
            }
            override fun onServiceLost(info: NsdServiceInfo) = onMain {
                if (!isCurrent(attemptGeneration)) return@onMain
                lostServices.add(info.serviceName)
                resolveQueue.removeIf { it.info.serviceName == info.serviceName }
                hintsByService.remove(info.serviceName)
                registeredServices.remove(info.serviceName)?.let(::withdrawPeerIfLastService)
            }
        }

    private fun failAttempt(attemptGeneration: Int, operation: String, detail: String?) {
        if (!isCurrent(attemptGeneration)) return
        Log.w(TAG, "NSD $operation failed: $detail")
        teardownAttempt()
        scheduleRetry()
    }

    private fun teardownAttempt() {
        if (!started && registrationListener == null && discoveryListener == null) return
        started = false
        registrationListener?.let { runCatching { nsd.unregisterService(it) } }
        discoveryListener?.let { runCatching { nsd.stopServiceDiscovery(it) } }
        registrationListener = null
        discoveryListener = null
        val peers = registeredServices.values.toSet()
        registeredServices.clear()
        hintsByService.clear()
        lostServices.clear()
        resolveQueue.clear()
        activeResolveToken = null
        worker.execute {
            peers.forEach { runCatching { engine.removeInternetLanPeer(it) } }
            runCatching { engine.stopInternetLanListener() }
            listenerLease.clear()
        }
        if (multicastLock.isHeld) multicastLock.release()
    }

    private fun scheduleRetry() {
        if (!desired) return
        val attempt = retryAttempt.coerceAtMost(5)
        retryAttempt += 1
        val retryGeneration = generation
        val delayMs = (RETRY_INITIAL_MS shl attempt).coerceAtMost(RETRY_MAX_MS)
        handler.postDelayed({ if (desired && generation == retryGeneration) startAttempt() }, delayMs)
    }

    private fun resolveNext() {
        if (!started || activeResolveToken != null) return
        val request = resolveQueue.poll() ?: return
        if (!isCurrent(request.generation) || lostServices.contains(request.info.serviceName)) {
            resolveNext()
            return
        }
        val token = ++nextResolveToken
        activeResolveToken = token
        runCatching { nsd.resolveService(request.info, makeResolveListener(request.generation, token)) }
            .onFailure {
                Log.w(TAG, "NSD resolve call failed: ${it.message}")
                finishResolve(request.generation, token)
            }
    }

    private fun makeResolveListener(attemptGeneration: Int, token: Int) =
        object : NsdManager.ResolveListener {
            override fun onResolveFailed(info: NsdServiceInfo, code: Int) = onMain {
                Log.w(TAG, "NSD resolve failed: error $code for ${info.serviceName}")
                finishResolve(attemptGeneration, token)
            }
            override fun onServiceResolved(info: NsdServiceInfo) = onMain {
                try {
                    if (!isCurrent(attemptGeneration) || lostServices.contains(info.serviceName)) return@onMain
                    val host = info.host ?: return@onMain
                    if (info.port !in 1..65535) return@onMain
                    val address = socketAddress(host, info.port)
                    val version = info.attributes["v"]?.toString(Charsets.UTF_8)
                    val peerId = info.attributes["id"]?.toString(Charsets.UTF_8)?.lowercase()
                    if (version != "1" || peerId == null || !PEER_ID.matches(peerId) || peerId == nodeIdHex) {
                        Log.w(TAG, "NSD rejected invalid/self TXT metadata at $address")
                        return@onMain
                    }
                    LanHint(peerId, address).also {
                        hintsByService[info.serviceName] = it
                        submitHint(info.serviceName, it, attemptGeneration)
                    }
                } finally {
                    finishResolve(attemptGeneration, token)
                }
            }
        }

    private fun submitHint(serviceName: String, hint: LanHint, attemptGeneration: Int) {
        worker.execute {
            val result = runCatching { engine.addInternetLanPeer(hint.peerId, hint.address) }
            handler.post {
                if (!isCurrent(attemptGeneration) || lostServices.contains(serviceName) ||
                    hintsByService[serviceName] != hint
                ) {
                    if (result.isSuccess) withdrawPeerIfLastService(hint.peerId)
                    return@post
                }
                result.onSuccess {
                    val previous = registeredServices.put(serviceName, hint.peerId)
                    if (previous != null && previous != hint.peerId) withdrawPeerIfLastService(previous)
                }.onFailure { Log.i(TAG, "LAN hint pending trust/activation: ${it.message}") }
            }
        }
    }

    private fun finishResolve(attemptGeneration: Int, token: Int) {
        if (activeResolveToken != token) return
        activeResolveToken = null
        if (isCurrent(attemptGeneration)) resolveNext()
    }

    private fun withdrawPeerIfLastService(peerId: String) {
        if (registeredServices.containsValue(peerId)) return
        worker.execute { runCatching { engine.removeInternetLanPeer(peerId) } }
    }

    private fun isCurrent(attemptGeneration: Int) =
        desired && started && generation == attemptGeneration

    private fun onMain(block: () -> Unit) {
        if (Looper.myLooper() == Looper.getMainLooper()) block() else handler.post(block)
    }

    companion object {
        private const val TAG = "IrisNsd"
        const val SERVICE_TYPE = "_iris._tcp."
        private const val RETRY_INITIAL_MS = 1_000L
        private const val RETRY_MAX_MS = 30_000L
        private val PEER_ID = Regex("^[0-9a-f]{64}$")

        internal fun socketAddress(host: InetAddress, port: Int): String {
            val hostAddress = requireNotNull(host.hostAddress)
            return if (host is Inet6Address) {
                val literal = hostAddress.substringBefore('%')
                val scoped = if (host.scopeId > 0) "$literal%${host.scopeId}" else literal
                "[$scoped]:$port"
            } else "$hostAddress:$port"
        }
    }
}
