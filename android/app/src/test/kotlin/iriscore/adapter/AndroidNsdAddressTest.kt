package iriscore.adapter

import java.net.InetAddress
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Test

class AndroidNsdAddressTest {
    @Test
    fun ipv4UsesHostColonPort() {
        assertEquals(
            "192.0.2.4:7890",
            AndroidNsdAdapter.socketAddress(InetAddress.getByName("192.0.2.4"), 7890),
        )
    }

    @Test
    fun ipv6IsBracketed() {
        assertEquals(
            "[0:0:0:0:0:0:0:1]:7890",
            AndroidNsdAdapter.socketAddress(InetAddress.getByName("::1"), 7890),
        )
    }

    @Test
    fun staleStartCannotReleaseNewListenerGeneration() {
        val lease = LanListenerLease()
        lease.acquired(1)
        lease.clear() // serialized stop for generation 1
        lease.acquired(2) // immediate restart completes

        assertFalse(lease.releaseIfOwned(1), "stale cleanup must not stop generation 2")
        assertTrue(lease.owns(2))
    }

    @Test
    fun currentGenerationCanReleaseItsOwnListener() {
        val lease = LanListenerLease()
        lease.acquired(7)

        assertTrue(lease.releaseIfOwned(7))
        assertFalse(lease.owns(7))
    }
}
