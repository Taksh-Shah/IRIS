package iriscore.adapter

/** Pure, host-testable reduction of Android capability flags. */
data class InternetNetworkState(
    val validatedWan: Boolean,
    val localNetwork: Boolean,
)

internal object InternetNetworkPolicy {
    fun reduce(
        hasInternetCapability: Boolean,
        hasValidatedCapability: Boolean,
        hasWifiTransport: Boolean,
        hasEthernetTransport: Boolean,
    ): InternetNetworkState = InternetNetworkState(
        validatedWan = hasInternetCapability && hasValidatedCapability,
        localNetwork = hasWifiTransport || hasEthernetTransport,
    )

    fun aggregate(defaultNetwork: InternetNetworkState, hasAnyLocalNetwork: Boolean) =
        InternetNetworkState(
            validatedWan = defaultNetwork.validatedWan,
            localNetwork = defaultNetwork.localNetwork || hasAnyLocalNetwork,
        )
}
