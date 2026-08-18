package iriscore.util

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * AC-6/AC-8 unit tests — OEM battery-kill guidance matrix (AC-7) + PeerId codec.
 */
class BatteryOptimizationGuidanceTest {

    @Test
    fun `matrix covers the major aggressive OEM roms`() {
        val vendors = BatteryOptimizationGuidance.matrix.map { it.vendor }
        assertTrue("samsung" in vendors)
        assertTrue("xiaomi" in vendors)
        assertTrue("oppo" in vendors)
        assertTrue("realme" in vendors)
        assertTrue("vivo" in vendors)
        assertTrue("google" in vendors)
        assertTrue("honor" in vendors)
    }

    @Test
    fun `isKnownVendor matches by substring`() {
        assertTrue(BatteryOptimizationGuidance.isKnownVendor("samsung"))
        assertTrue(BatteryOptimizationGuidance.isKnownVendor("Xiaomi"))
        assertFalse(BatteryOptimizationGuidance.isKnownVendor("nokia"))
    }

    @Test
    fun `guidanceFor falls back to generic android instructions`() {
        val g = BatteryOptimizationGuidance.guidanceFor("nokia")
        assertEquals("nokia", g.vendor)
        assertTrue(g.steps.any { "Unrestricted" in it })
    }
}

class PeerIdCodecTest {

    @Test
    fun `toHex then fromHex round trips`() {
        val raw = ByteArray(32) { it.toByte() }
        val hex = PeerIdCodec.toHex(raw)
        assertEquals(64, hex.length)
        assertEquals(raw.toList(), PeerIdCodec.fromHex(hex)!!.toList())
    }

    @Test
    fun `fromHex rejects malformed input`() {
        assertNull(PeerIdCodec.fromHex("xyz"))
        assertNull(PeerIdCodec.fromHex("abc"))
        assertNull(PeerIdCodec.fromHex("GG"))
    }

    @Test
    fun `shortId truncates to 16 chars`() {
        assertEquals("a1b2c3d4e5f6a7b8", PeerIdCodec.shortId("a1b2c3d4e5f6a7b8cd" + "0".repeat(48)))
    }
}