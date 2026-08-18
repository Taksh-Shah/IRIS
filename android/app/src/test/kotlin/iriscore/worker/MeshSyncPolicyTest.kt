package iriscore.worker

import java.time.Duration
import java.time.Instant
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class MeshSyncPolicyTest {

    private val now = Instant.parse("2026-08-17T12:00:00Z")

    @Test
    fun `cadence is 15 minutes with 5 minute flex`() {
        assertEquals(Duration.ofMinutes(15), MeshSyncPolicy.cadence())
        assertEquals(Duration.ofMinutes(5), MeshSyncPolicy.flex())
        assertEquals(15L, MeshSyncPolicy.CADENCE_MINUTES)
        assertEquals(5L, MeshSyncPolicy.CADENCE_FLEX_MINUTES)
    }

    @Test
    fun `not due before the first cadence`() {
        val lastRun = Instant.parse("2026-08-17T11:50:00Z") // 10 min ago
        assertFalse(MeshSyncPolicy.shouldRun(lastRun, now, batteryPercent = 80, isCharging = true))
    }

    @Test
    fun `due exactly at the cadence boundary`() {
        val lastRun = Instant.parse("2026-08-17T11:45:00Z") // 15 min ago
        assertTrue(MeshSyncPolicy.shouldRun(lastRun, now, batteryPercent = 80, isCharging = false))
    }

    @Test
    fun `due runs are suppressed at critical battery when not charging`() {
        val lastRun = Instant.parse("2026-08-17T11:45:00Z")
        assertFalse(MeshSyncPolicy.shouldRun(lastRun, now, batteryPercent = 8, isCharging = false))
    }

    @Test
    fun `critical battery while charging is allowed to run`() {
        val lastRun = Instant.parse("2026-08-17T11:45:00Z")
        assertTrue(MeshSyncPolicy.shouldRun(lastRun, now, batteryPercent = 8, isCharging = true))
    }

    @Test
    fun `nextRunDelay is zero once due`() {
        val lastRun = Instant.parse("2026-08-17T11:30:00Z") // 30 min ago
        assertEquals(Duration.ZERO, MeshSyncPolicy.nextRunDelay(lastRun, now))
    }

    @Test
    fun `nextRunDelay counts down to the boundary`() {
        val lastRun = Instant.parse("2026-08-17T11:50:00Z") // 10 min ago -> 5 min remaining
        assertEquals(Duration.ofMinutes(5), MeshSyncPolicy.nextRunDelay(lastRun, now))
    }
}