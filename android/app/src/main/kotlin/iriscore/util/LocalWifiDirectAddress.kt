package iriscore.util

/**
 * HV-21: this device's own Wi-Fi Direct (P2P) MAC, shared across adapters.
 *
 * `AndroidWifiDirectTransportAdapter` learns this from the platform's
 * `WIFI_P2P_THIS_DEVICE_CHANGED_ACTION` broadcast — it already needed the
 * value for the post-connect handshake (FFI-3). `AndroidBleTransportAdapter`
 * needs the *same* value to publish in its outgoing beacon, so a peer we've
 * already identified over BLE can be matched to its Wi-Fi Direct device once
 * plain `discoverPeers()` finds it (HV-21's fix: DNS-SD never resolves on
 * this hardware, but the BLE control-plane channel already works, so it
 * carries the P2P identity instead of a fragile P2P TXT record).
 *
 * A tiny volatile holder is deliberately simpler than a DI-provided
 * interface here: both adapters are process-wide singletons already (Hilt
 * `@Singleton` via the engine's adapter set), the value has no lifecycle of
 * its own beyond "the platform hasn't told us yet" (null), and it is only
 * ever read opportunistically (a beacon built before the address is known
 * simply omits it — see [iriscore.adapter.AndroidBleTransportAdapter]).
 */
object LocalWifiDirectAddress {
    @Volatile
    var address: String? = null

    /**
     * Set by whoever owns the [iriscode.IrisEngine] instance (the Hilt
     * `IrisCoreModule` in production, the Mobly snippet in the bench
     * harness) — invoked every time [address] changes so the engine can be
     * told via `setLocalWifiDirectMac`. A plain callback rather than a
     * `Flow`/`StateFlow` because there is exactly one process-wide engine
     * and the update is fire-and-forget (the engine call itself is cheap
     * and idempotent).
     */
    @Volatile
    var onChanged: ((String) -> Unit)? = null
}
