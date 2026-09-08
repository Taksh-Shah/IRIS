package iriscore.data

import java.util.concurrent.atomic.AtomicInteger
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
 *
 * **HV-38 naming note — this is NOT the mesh's relay queue.** This class holds
 * only messages *this node originated* that the FFI `sendText` call itself
 * rejected; it is a local retry spool for send failures, not the core relay
 * path. The core's actual mesh relay — forwarding *other peers'* messages
 * toward their destination (`message_engine::enqueue_relay`, counted in
 * `metrics.relayed`) — is a completely separate mechanism with no Kotlin
 * involvement at all; it never touches this class. `MeshUiState.relayQueued`
 * (driven by [size]) reflects only this local spool, which is why `/peers`
 * shows it labeled "pending sends (local)" next to the real, distinctly
 * labeled mesh-relay counter, not as one ambiguous "relay" number. A more
 * thorough fix renames this class to `PendingSendQueue` — not done here to
 * keep this change's diff reviewable (a rename touches DI wiring and every
 * call site); left as a documented follow-up.
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

    /**
     * Published size. `ArrayDeque` is not thread-safe and [size] is read from
     * the UI without holding [mutex], so the count is mirrored into an atomic
     * that is updated under the lock rather than reading the deque directly.
     */
    private val count = AtomicInteger(0)

    /** @return `true` if accepted, `false` if dropped (overflow at same sender pressure). */
    suspend fun enqueue(message: QueuedMessage): Boolean = mutex.withLock {
        if (queue.size >= capacity) {
            queue.removeFirst()
        }
        queue.addLast(message)
        count.set(queue.size)
        true
    }

    val size: Int get() = count.get()

    /** Drop locally queued sends for a peer the operator has forgotten. */
    suspend fun removeRecipient(recipientHex: String): Int = mutex.withLock {
        val before = queue.size
        queue.removeAll { it.recipientHex.equals(recipientHex, ignoreCase = true) }
        count.set(queue.size)
        before - queue.size
    }

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
        count.set(queue.size)
        sent
    }

    companion object {
        const val DEFAULT_CAPACITY = 256
    }
}
