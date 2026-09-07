package iriscore.adapter

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import iriscode.IrisEngine
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Tier-5 connectivity bridge. Android owns network validity; Rust owns TCP
 * framing/retry. This monitor never treats a network callback as proof that a
 * peer or relay is reachable.
 */
class AndroidInternetNetworkMonitor(
    context: Context,
    private val engine: IrisEngine,
    relayEndpointsCsv: String,
) {
    private val connectivity = context.getSystemService(ConnectivityManager::class.java)
    private val endpoints = relayEndpointsCsv.split(',').map(String::trim).filter(String::isNotEmpty)
    private val started = AtomicBoolean(false)

    private val callback = object : ConnectivityManager.NetworkCallback() {
        override fun onAvailable(network: Network) = publish(network)
        override fun onCapabilitiesChanged(network: Network, capabilities: NetworkCapabilities) = publish(network)
        override fun onLost(network: Network) = engine.setInternetNetworkAvailable(false)
    }

    fun start() {
        if (!started.compareAndSet(false, true)) return
        // Configuration is idempotent and intentionally performed before the
        // callback registration: an online device becomes Available only when
        // it has both a validated network and a relay endpoint.
        engine.setInternetRelayEndpoints(endpoints)
        connectivity.registerDefaultNetworkCallback(callback)
        connectivity.activeNetwork?.let(::publish)
    }

    /** Releases the platform callback and withdraws the availability hint. */
    fun stop() {
        if (!started.compareAndSet(true, false)) return
        runCatching { connectivity.unregisterNetworkCallback(callback) }
        engine.setInternetNetworkAvailable(false)
    }

    private fun publish(network: Network) {
        val caps = connectivity.getNetworkCapabilities(network)
        val usable = caps?.hasCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET) == true &&
            caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED)
        engine.setInternetNetworkAvailable(usable)
    }
}
