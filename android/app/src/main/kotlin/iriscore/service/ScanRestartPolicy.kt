package iriscore.service

/**
 * BLE scan-restart cadence policy (AC-7). Pure JVM — unit-tested on the host
 * Gradle `test` leg.
 *
 * Enforcement pattern (BLE-001): bounded active-scan windows with an idle
 * gap, plus a minimum cooldown after an empty window so a quiet radio doesn't
 * keep the BLE radio awake. Android battery doc / OEM doze tolerance both
 * reward "bursty" scans over continuous ones.
 */
object ScanRestartPolicy {

    /** Active scan window: 30 s on. */
    const val WINDOW_ON_MS = 30_000L

    /** Regular idle gap between windows: 30 s off. */
    const val WINDOW_OFF_MS = 30_000L

    /** Floor cooldown after an empty scan window: >= 5 s (border ≥ 5/30 s). */
    const val MIN_COOLDOWN_MS = 5_000L

    /** Extra backoff added per consecutive empty window (capped). */
    const val EMPTY_PENALTY_MS = 30_000L

    const val MAX_CONSECUTIVE_EMPTIES = 4

    /** Idle gap before the next window. */
    fun offMs(consecutiveEmpties: Int): Long {
        require(consecutiveEmpties >= 0) { "consecutiveEmpties must be >= 0" }
        if (consecutiveEmpties == 0) return WINDOW_OFF_MS
        val penalty = EMPTY_PENALTY_MS *
            consecutiveEmpties.coerceAtMost(MAX_CONSECUTIVE_EMPTIES)
        return (MIN_COOLDOWN_MS + penalty).coerceAtMost(WINDOW_OFF_MS + 4 * EMPTY_PENALTY_MS)
    }

    /** Cooldown (>= floor) after an empty window, for stream/UI readout. */
    fun cooldownMs(consecutiveEmpties: Int): Long =
        (MIN_COOLDOWN_MS + EMPTY_PENALTY_MS * consecutiveEmpties.coerceAtMost(MAX_CONSECUTIVE_EMPTIES))
            .coerceAtLeast(MIN_COOLDOWN_MS)
}