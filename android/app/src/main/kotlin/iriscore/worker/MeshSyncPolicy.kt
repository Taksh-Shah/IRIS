package iriscore.worker

import java.time.Duration
import java.time.Instant

/**
 * Mesh background-sync policy (AC-7) — pure JVM, unit-tested on the host Gradle
 * `test` leg. Encodes the 15-min convention (ANDROID.md) plus battery-aware
 * guards: a critical-battery, non-charging device defers relay work to the next
 * cadence rather than waking the radio.
 */
object MeshSyncPolicy {

    /** WorkManager periodic cadence (min). */
    const val CADENCE_MINUTES = 15L

    /** Flex interval so the platform can coalesce wakes (Doze-friendly). */
    const val CADENCE_FLEX_MINUTES = 5L

    const val CRITICAL_BATTERY_PCT = 15

    fun cadence(): Duration = Duration.ofMinutes(CADENCE_MINUTES)

    fun flex(): Duration = Duration.ofMinutes(CADENCE_FLEX_MINUTES)

    /**
     * Whether a relay pass is due. Battery-aware: skips while battery is
     * critically low AND not charging.
     */
    fun shouldRun(
        lastRunAt: Instant,
        now: Instant,
        batteryPercent: Int,
        isCharging: Boolean,
    ): Boolean {
        val elapsed = Duration.between(lastRunAt, now).toMinutes()
        val due = elapsed >= CADENCE_MINUTES
        val batteryOk = isCharging || batteryPercent >= CRITICAL_BATTERY_PCT
        return due && batteryOk
    }

    /** Delay until the next due run (for observability/tests). */
    fun nextRunDelay(lastRunAt: Instant, now: Instant): Duration {
        val elapsed = Duration.between(lastRunAt, now)
        val due = Duration.ofMinutes(CADENCE_MINUTES).minus(elapsed)
        return if (due.isNegative || due.isZero) Duration.ZERO else due
    }
}