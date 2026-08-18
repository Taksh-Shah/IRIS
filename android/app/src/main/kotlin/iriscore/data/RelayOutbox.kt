package iriscore.data

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * In-memory outbound relay spool (AC-7 immutable store-and-forward seam).
 *
 * The Rust engine keeps authoritative durable state (`MemoryStorage` today;
 * Room metadata store at maturity); this outbox holds outbound P0/P1 texts queued
 * while no transport is up, drained by [MeshRepository.drainRelayOutbox] (the
 * 15-min WorkManager cadence path and on engine start).
 *
 * Bounded — drops the oldest entry past the cap so a stale destination can't
 * grow the queue unboundedly (mirrors NEW-WA-RT-112 ring-buffer policy).
 */
class RelayOutbox(
    private val capacity: Int = DEFAULT_CAPACITY,
) {

    data class QueuedMessage(
        val recipientHex: String,
        val text: String,
        val priority: UByte,
    )

    private val queue = ArrayDeque<QueuedMessage>()
    private val mutex = Mutex()

    /** @return `true` if accepted, `false` if dropped (overflow at same sender pressure). */
    suspend fun enqueue(message: QueuedMessage): Boolean = mutex.withLock {
        if (queue.size >= capacity) {
            queue.removeFirst()
            queue.addLast(message)
            true
        } else {
            queue.addLast(message)
            true
        }
    }

    val size: Int get() = queue.size

    /** Drain up to [maxItems] applying [drain]. Items are consumed in FIFO order. */
    suspend fun drain(
        maxItems: Int = Int.MAX_VALUE,
        drain: suspend (QueuedMessage) -> Boolean,
    ): Int = mutex.withLock {
        var sent = 0
        while (queue.isNotEmpty() && sent < maxItems) {
            val next = queue.removeFirst()
            if (drain(next)) sent++ else {
                // Re-enqueue failed item at the tail (retry next cadence)
                queue.addLast(next)
                break
            }
        }
        sent
    }

    companion object {
        const val DEFAULT_CAPACITY = 256
    }
}