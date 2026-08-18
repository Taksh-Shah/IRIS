package iriscore.service

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * AC-7 unit tests — scan-restart cadence policy (BLE-001 5/30 s pattern).
 */
class ScanRestartPolicyTest {

    @Test
    fun `regular cadence idles 30 s between windows`() {
        assertEquals(30_000L, ScanRestartPolicy.WINDOW_ON_MS)
        assertEquals(30_000L, ScanRestartPolicy.offMs(consecutiveEmpties = 0))
    }

    @Test
    fun `empty window respects the 5 s cooldown floor and backs off`() {
        val first = ScanRestartPolicy.cooldownMs(1)
        assertTrue(first >= ScanRestartPolicy.MIN_COOLDOWN_MS, "cooldown must be >= 5s floor")
        val second = ScanRestartPolicy.cooldownMs(2)
        assertTrue(second > first, "backoff grows with consecutive empties")
    }

    @Test
    fun `backoff converges on a bounded ceiling`() {
        assertEquals(
            ScanRestartPolicy.cooldownMs(ScanRestartPolicy.MAX_CONSECUTIVE_EMPTIES),
            ScanRestartPolicy.cooldownMs(ScanRestartPolicy.MAX_CONSECUTIVE_EMPTIES + 9),
        )
        val ceiling = 30_000L + 4 * ScanRestartPolicy.EMPTY_PENALTY_MS
        assertTrue(
            ScanRestartPolicy.offMs(ScanRestartPolicy.MAX_CONSECUTIVE_EMPTIES) <= ceiling,
        )
    }

    @Test
    fun `a successful window resets the backoff`() {
        val backedOff = ScanRestartPolicy.offMs(3)
        assertTrue(backedOff > ScanRestartPolicy.offMs(0))
    }
}