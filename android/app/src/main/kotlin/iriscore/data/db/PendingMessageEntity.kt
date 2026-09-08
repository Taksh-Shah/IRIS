package iriscore.data.db

import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import iriscore.data.RelayOutbox

/**
 * Durable pending-send row — persists the [RelayOutbox] queue across process
 * death and reboots. Rows are deleted when the engine successfully delivers the
 * message; rows that survive a restart are retried on the next [startMesh].
 *
 * WP11: [messageUid] links this pending-send row back to the [MessageEntity]
 * row the user actually sees, so a successful drain can flip that row's
 * status from QUEUED to DELIVERED instead of leaving it stuck at QUEUED
 * forever (the message list and the relay outbox used to be two entirely
 * disconnected tables — this was the root cause of "queued rows never
 * transition after the queue drains"). [attempts] bounds how many drain
 * passes a row survives before the outbox gives up on it; on that final
 * failure [MeshRepository.drainRelayOutbox] marks the message row FAILED and
 * deletes this row, rather than retrying it forever every 15 minutes.
 */
@Entity(
    tableName = "pending_messages",
    indices = [Index("recipientHex"), Index("enqueuedAtMs")],
)
data class PendingMessageEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val messageUid: Long,
    val recipientHex: String,
    val text: String,
    val priority: Int,
    val enqueuedAtMs: Long,
    val attempts: Int = 0,
) {
    fun toQueued(): RelayOutbox.QueuedMessage =
        RelayOutbox.QueuedMessage(messageUid, recipientHex, text, priority.toUByte())
}
