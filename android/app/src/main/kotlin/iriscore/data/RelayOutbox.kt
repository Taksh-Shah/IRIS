package iriscore.data

import iriscore.data.db.PendingMessageEntity
import iriscore.data.db.PendingMessageDao
import kotlinx.coroutines.flow.Flow
import java.util.concurrent.atomic.AtomicInteger

/**
 * Durable outbound relay spool (AC-7 store-and-forward seam).
 *
 * Backed by [PendingMessageDao] (Room) so queued sends survive process death
 * and device reboot. Rows are enqueued when the engine rejects a send, and
 * deleted when the next drain attempt succeeds.
 *
 * The in-memory [count] mirror is updated on every mutation so [size] stays
 * fast without a DB round-trip; it is re-synchronised on first read from
 * [observeCount] (a Room Flow).
 *
 * HV-38 naming note: this class holds ONLY messages this node originated that
 * the FFI sendText call rejected. It is a local retry spool, not the mesh's
 * relay mechanism (which forwards other peers' messages entirely in Rust).
 */
class RelayOutbox(private val dao: PendingMessageDao) {

    data class QueuedMessage(
        val recipientHex: String,
        val text: String,
        val priority: UByte,
    )

    private val count = AtomicInteger(0)

    val size: Int get() = count.get()

    /** Live count Flow for the [iriscore.ui.state.MeshUiState.relayQueued] badge. */
    fun observeCount(): Flow<Int> = dao.observeCount()

    /** Persist a message for later delivery. Always succeeds (no cap drop). */
    suspend fun enqueue(message: QueuedMessage): Boolean {
        dao.insert(
            PendingMessageEntity(
                recipientHex = message.recipientHex,
                text = message.text,
                priority = message.priority.toInt(),
                enqueuedAtMs = System.currentTimeMillis(),
            ),
        )
        count.incrementAndGet()
        return true
    }

    /** Drop locally queued sends for a peer the operator has forgotten. */
    suspend fun removeRecipient(recipientHex: String): Int {
        val before = count.get()
        dao.removeRecipient(recipientHex)
        // Approximate: exact count comes from observeCount(); this just keeps the
        // size() fast for the immediate UI feedback after a forget operation.
        val removed = dao.getAll().let { before - it.size }.coerceAtLeast(0)
        count.set(dao.getAll().size)
        return removed
    }

    /**
     * Drain all pending messages applying [drain] to each. Items are processed
     * in FIFO order (earliest-enqueued first). A [drain] that returns false
     * stops the loop — the remaining items stay in the database for the next
     * cadence.
     *
     * @return number of messages successfully delivered and deleted.
     */
    suspend fun drain(
        maxItems: Int = Int.MAX_VALUE,
        drain: suspend (QueuedMessage) -> Boolean,
    ): Int {
        val all = dao.getAll().take(maxItems)
        var sent = 0
        for (entity in all) {
            if (!drain(entity.toQueued())) break
            dao.delete(entity)
            sent++
        }
        count.set(dao.getAll().size)
        return sent
    }
}
