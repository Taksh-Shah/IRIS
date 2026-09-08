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
 * deleted when the next drain attempt succeeds — or when a row exceeds
 * [MAX_DRAIN_ATTEMPTS] and the caller gives up on it (see [drain]'s
 * `onGiveUp`).
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
        val messageUid: Long,
        val recipientHex: String,
        val text: String,
        val priority: UByte,
    )

    /**
     * WP11: how many failed drain passes a row survives before the caller is
     * told to give up on it (see [drain]'s `onGiveUp`) rather than retrying
     * it forever on every 15-minute WorkManager cadence.
     */
    companion object {
        const val MAX_DRAIN_ATTEMPTS = 5
    }

    private val count = AtomicInteger(0)

    val size: Int get() = count.get()

    /** Live count Flow for the [iriscore.ui.state.MeshUiState.relayQueued] badge. */
    fun observeCount(): Flow<Int> = dao.observeCount()

    /** Persist a message for later delivery. Always succeeds (no cap drop). */
    suspend fun enqueue(message: QueuedMessage): Boolean {
        dao.insert(
            PendingMessageEntity(
                messageUid = message.messageUid,
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

    /** WP12: drop the outbox row for one deleted message, without touching
     * any other pending send to the same recipient. */
    suspend fun removeByMessageUid(messageUid: Long) {
        dao.removeByMessageUid(messageUid)
        count.set(dao.getAll().size)
    }

    /**
     * Drain all pending messages, applying [onSend] to each in FIFO order
     * (earliest-enqueued first). A row that fails [onSend] is not deleted:
     * its attempt count is incremented, and once it exceeds
     * [MAX_DRAIN_ATTEMPTS], [onGiveUp] is called (typically to mark the
     * corresponding message row FAILED) and the row is deleted so it stops
     * being retried. A row that has not yet exceeded the cap stays queued
     * and the loop moves on to the next row rather than stopping the whole
     * drain — one bad recipient no longer starves every other queued send.
     *
     * @return number of messages successfully delivered and deleted.
     */
    suspend fun drain(
        maxItems: Int = Int.MAX_VALUE,
        onSend: suspend (QueuedMessage) -> Boolean,
        onGiveUp: suspend (messageUid: Long) -> Unit,
    ): Int {
        val all = dao.getAll().take(maxItems)
        var sent = 0
        for (entity in all) {
            if (onSend(entity.toQueued())) {
                dao.delete(entity)
                sent++
            } else if (entity.attempts + 1 >= MAX_DRAIN_ATTEMPTS) {
                onGiveUp(entity.messageUid)
                dao.delete(entity)
            } else {
                dao.incrementAttempts(entity.id)
            }
        }
        count.set(dao.getAll().size)
        return sent
    }
}
