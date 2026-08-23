package iriscore.adapter

import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.TimeoutCancellationException
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * AC-5 unit tests — one per carry-forward requirement (ANDROID_DESIGN.md §5/§7):
 * RT-110 per-call timeouts, RT-111/RT-010 idempotent start/subscribe,
 * RT-112 ring-buffer outbox, RT-109 closed-NDP prune + multi-subscriber
 * availability stream, RT-108 verified-peer reuse key.
 *
 * Pure-JVM (kotlinx.coroutines + JUnit5); runs in the Gradle `test` leg. The
 * Gradle/Android compile leg is environment-gated on the integration host.
 */
class AdapterLifecycleTest {

    // -- NEW-WA-RT-110 ------------------------------------------------------

    @Test
    fun `rt110 suspendCall times out with TimeoutCancellationException`() {
        // assertThrows takes a non-suspend Executable, so the suspend call has
        // to be driven by its own runBlocking inside the lambda.
        assertThrows(TimeoutCancellationException::class.java) {
            runBlocking { FfiCallTimeout.suspendCall(timeoutMs = 20L) { delay(5_000L) } }
        }
    }

    @Test
    fun `rt110 syncCall returns onTimeout fallback when the platform blocks`() {
        val result = FfiCallTimeout.syncCall(timeoutMs = 20L, onTimeout = -1) {
            Thread.sleep(5_000L)
            42
        }
        assertEquals(-1, result)
    }

    @Test
    fun `rt110 syncCall returns the value on fast completion`() {
        val result = FfiCallTimeout.syncCall(timeoutMs = 500L, onTimeout = -1) { 7 }
        assertEquals(7, result)
    }

    // -- NEW-WA-RT-111 / WIFIDIRECT RT-010 ---------------------------------

    @Test
    fun `rt111 ensureStarted is idempotent - create runs exactly once`() = runBlocking {
        var creates = 0
        val gate = SessionGate { creates++; "session" }
        val first = gate.ensureStarted()
        val second = gate.ensureStarted()
        val third = gate.ensureStarted()
        assertEquals("session", first)
        assertEquals(first, second)
        assertEquals(first, third)
        assertEquals(1, creates) // same-session reuse
    }

    @Test
    fun `rt111 ensureStarted never latches into failure - retry recovers`() = runBlocking {
        var attempts = 0
        val gate = SessionGate<Unit> {
            attempts++
            if (attempts == 1) throw IllegalStateException("platform down")
            Unit
        }
        assertThrows(IllegalStateException::class.java) { runBlocking { gate.ensureStarted() } }
        gate.ensureStarted() // second attempt must retry, not reuse a failed latch
        assertEquals(2, attempts)
    }

    @Test
    fun `rt111 concurrent starters coalesce into one create`() = runBlocking {
        var creates = 0
        val gate = SessionGate<Unit> {
            creates++
            delay(20L)
            Unit
        }
        coroutineScope {
            val jobs = (0 until 8).map { async { gate.ensureStarted() } }
            jobs.awaitAll()
        }
        assertEquals(1, creates)
    }

    @Test
    fun `rt111 markStarted seeds async resource and reset drops it`() = runBlocking {
        val gate = SessionGate<Unit> { Unit }
        assertFalse(gate.isStarted())
        assertTrue(gate.markStarted(Unit)) // onSessionStarted path
        assertTrue(gate.isStarted())
        assertFalse(gate.markStarted(Unit)) // never downgrades a live resource
        assertTrue(gate.reset())
        assertFalse(gate.isStarted())
    }

    @Test
    fun `rt010 invalidate clears the latch without blocking`() = runBlocking {
        val gate = SessionGate<Unit> { Unit }
        gate.ensureStarted()
        gate.invalidate() // channel-lost callback
        assertFalse(gate.isStarted())
        gate.ensureStarted() // re-initializes
        assertTrue(gate.isStarted())
    }

    // -- NEW-WA-RT-112 ------------------------------------------------------

    @Test
    fun `rt112 outbox drops oldest for the same destination only`() {
        val outbox = RingBufferOutbox<Int>(capacityPerDestination = 3, maxDestinations = 8)
        outbox.enqueue(1, 10)
        outbox.enqueue(1, 11)
        outbox.enqueue(1, 12)
        outbox.enqueue(1, 13) // overflow: oldest of dest 1 dropped
        assertEquals(listOf(11, 12, 13), outbox.drain(1))
    }

    @Test
    fun `rt112 outbox keeps other destinations when one saturates`() {
        val outbox = RingBufferOutbox<Int>(capacityPerDestination = 2, maxDestinations = 8)
        outbox.enqueue(1, 10)
        outbox.enqueue(2, 20)
        outbox.enqueue(2, 21)
        repeat(5) { outbox.enqueue(1, 10 + it) } // dest 1 saturates
        // dest 2 is untouched by dest-1 overflow (per-destination eviction).
        assertEquals(listOf(20, 21), outbox.drain(2))
        // dest 1 oldest kept at cap (10, 11, 12, 13, 14 -> 13, 14).
        assertEquals(2, outbox.sizeFor(1))
        assertEquals(listOf(13, 14), outbox.drain(1))
    }

    @Test
    fun `rt112 drain returns frames once and removes the destination`() {
        val outbox = RingBufferOutbox<Int>(capacityPerDestination = 4, maxDestinations = 8)
        outbox.enqueue(5, 50)
        outbox.enqueue(5, 51)
        assertEquals(listOf(50, 51), outbox.drain(5))
        assertEquals(emptyList<Int>(), outbox.drain(5))
        assertEquals(0, outbox.totalSize())
    }

    // -- NEW-WA-RT-109 ------------------------------------------------------

    @Test
    fun `rt109 closed ndp frames are pruned at the drain boundary`() {
        val registry = NdpRegistry()
        registry.opened(1L)
        registry.opened(2L)
        assertTrue(registry.accepts(1L))
        registry.closed(1L)
        assertFalse(registry.accepts(1L)) // closed-NDP prune
        assertTrue(registry.accepts(2L))
        assertEquals(setOf(2L), registry.openHandles())
        assertEquals(listOf(2L), registry.closeAll())
        assertEquals(0, registry.openCount())
    }

    @Test
    fun `rt109 availability is a multi-subscriber replay stream`() = runBlocking {
        val tracker = RadioStateTracker()
        tracker.setAvailable(true)
        // Second subscriber added AFTER the transition still sees current state (replay).
        val late = tracker.available.first()
        assertTrue(late)
        tracker.setAvailable(false)
        assertEquals(false, tracker.available.first())
        assertFalse(tracker.isAvailable())
    }

    // -- NEW-WA-RT-108 ------------------------------------------------------

    @Test
    fun `rt108 verified peer reuse key resolves after envelope resolution`() {
        val cache = VerifiedPeerCache()
        cache.rememberVerified(platformHandle = 7L, verifiedPeerIdHex = "ab".repeat(32))
        assertEquals("ab".repeat(32), cache.verifiedPeerIdFor(7L))
        assertEquals(7L, cache.reuseHandleFor("ab".repeat(32)))
        cache.evict(7L)
        assertNull(cache.verifiedPeerIdFor(7L))
        assertNull(cache.reuseHandleFor("ab".repeat(32)))
    }
}