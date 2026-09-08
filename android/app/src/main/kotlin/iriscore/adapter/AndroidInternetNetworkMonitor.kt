package iriscore.adapter

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import iriscode.IrisEngine
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/** Tracks validated default-WAN and every usable Wi-Fi/Ethernet LAN separately. */
class AndroidInternetNetworkMonitor(
    context: Context,
    private val engine: IrisEngine,
    relayEndpointsCsv: String,
    private val relayServerName: String,
) {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private val endpoints = relayEndpointsCsv.split(',').map(String::trim).filter(String::isNotEmpty)
    private val started = AtomicBoolean(false)
    private val defaultNetwork = AtomicReference<Network?>(null)
    private val defaultState = AtomicReference(InternetNetworkState(false, false))
    private val localNetworks = ConcurrentHashMap<Network, Boolean>()
    private val published = AtomicReference(InternetNetworkState(false, false))

    /** Invoked after each distinct aggregate transition; consumers must offload I/O. */
    @Volatile var onNetworkStateChanged: ((InternetNetworkState) -> Unit)? = null

    private val defaultCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) {
            if (started.get()) defaultNetwork.set(network)
        }

        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
            if (!started.get() || defaultNetwork.get() != network) return
            defaultState.set(reduce(capabilities))
            publishAggregate()
        }

        override fun onLost(network: Network) {
            if (!started.get() || !defaultNetwork.compareAndSet(network, null)) return
            defaultState.set(InternetNetworkState(false, false))
            publishAggregate()
        }
    }

    private val localCallback = object : ConnectivityManager.NetworkCallback() {
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) {
            if (!started.get()) return
            val state = reduce(capabilities)
            if (state.localNetwork) localNetworks[network] = true else localNetworks.remove(network)
            publishAggregate()
        }

        override fun onLost(network: Network) {
            if (!started.get()) return
            localNetworks.remove(network)
            publishAggregate()
        }
    }

    fun start() {
        if (!started.compareAndSet(false, true)) return
        try {
            engine.configureInternetRelay(endpoints, relayServerName)
            connectivity.registerDefaultNetworkCallback(defaultCallback)
            val localRequest = NetworkRequest.Builder()
                .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
                .addTransportType(NetworkCapabilities.TRANSPORT_ETHERNET)
                .build()
            connectivity.registerNetworkCallback(localRequest, localCallback)
        } catch (error: Exception) {
            started.set(false)
            runCatching { connectivity.unregisterNetworkCallback(defaultCallback) }
            runCatching { connectivity.unregisterNetworkCallback(localCallback) }
            publishOffline()
            throw error
        }
    }

    /** Releases both callbacks and withdraws all transport hints. */
    fun stop() {
        if (!started.compareAndSet(true, false)) return
        runCatching { connectivity.unregisterNetworkCallback(defaultCallback) }
        runCatching { connectivity.unregisterNetworkCallback(localCallback) }
        defaultNetwork.set(null)
        defaultState.set(InternetNetworkState(false, false))
        localNetworks.clear()
        publishOffline()
    }

    fun isValidatedWanAvailable(): Boolean = published.get().validatedWan

    fun currentState(): InternetNetworkState = published.get()

    private fun reduce(caps: NetworkCapabilities): InternetNetworkState =
        InternetNetworkPolicy.reduce(
            hasInternetCapability = caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET),
            hasValidatedCapability = caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED),
            hasWifiTransport = caps.hasTransport(NetworkCapabilities.TRANSPORT_WIFI),
            hasEthernetTransport = caps.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET),
        )

    private fun publishAggregate() {
        val aggregate = InternetNetworkPolicy.aggregate(defaultState.get(), localNetworks.isNotEmpty())
        val previous = published.getAndSet(aggregate)
        if (previous == aggregate) return
        onNetworkStateChanged?.invoke(aggregate)
    }

    private fun publishOffline() {
        val offline = InternetNetworkState(false, false)
        published.set(offline)
        onNetworkStateChanged?.invoke(offline)
    }
}
