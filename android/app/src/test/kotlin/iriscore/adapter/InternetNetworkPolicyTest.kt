package iriscore.adapter

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

class InternetNetworkPolicyTest {
    @Test
    fun internetCapabilityWithoutValidationIsNotWan() {
        assertEquals(
            InternetNetworkState(validatedWan = false, localNetwork = false),
            InternetNetworkPolicy.reduce(true, false, false, false),
        )
    }

    @Test
    fun isolatedWifiRemainsAUsableLocalNetwork() {
        assertEquals(
            InternetNetworkState(validatedWan = false, localNetwork = true),
            InternetNetworkPolicy.reduce(false, false, true, false),
        )
    }

    @Test
    fun validatedCellularIsWanButNotLan() {
        assertEquals(
            InternetNetworkState(validatedWan = true, localNetwork = false),
            InternetNetworkPolicy.reduce(true, true, false, false),
        )
    }

    @Test
    fun validatedWifiSupportsBothPaths() {
        assertEquals(
            InternetNetworkState(validatedWan = true, localNetwork = true),
            InternetNetworkPolicy.reduce(true, true, true, false),
        )
    }

    @Test
    fun nonDefaultWifiKeepsLanUpBesideCellularWan() {
        assertEquals(
            InternetNetworkState(validatedWan = true, localNetwork = true),
            InternetNetworkPolicy.aggregate(
                InternetNetworkState(validatedWan = true, localNetwork = false),
                hasAnyLocalNetwork = true,
            ),
        )
    }

    @Test
    fun losingLastLocalNetworkDoesNotDropValidatedWan() {
        assertEquals(
            InternetNetworkState(validatedWan = true, localNetwork = false),
            InternetNetworkPolicy.aggregate(
                InternetNetworkState(validatedWan = true, localNetwork = false),
                hasAnyLocalNetwork = false,
            ),
        )
    }
}
