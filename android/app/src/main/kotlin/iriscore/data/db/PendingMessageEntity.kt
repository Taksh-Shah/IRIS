package iriscore.data.db

import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import iriscore.data.RelayOutbox

/**
 * Durable pending-send row — persists the [RelayOutbox] queue across process
 * death and reboots. Rows are deleted when the engine successfully delivers the
 * message; rows that survive a restart are retried on the next [startMesh].
 */
@Entity(
    tableName = "pending_messages",
    indices = [Index("recipientHex"), Index("enqueuedAtMs")],
)
data class PendingMessageEntity(
    @PrimaryKey(autoGenerate = true) val id: Long = 0,
    val recipientHex: String,
    val text: String,
    val priority: Int,
    val enqueuedAtMs: Long,
) {
    fun toQueued(): RelayOutbox.QueuedMessage =
        RelayOutbox.QueuedMessage(recipientHex, text, priority.toUByte())
}
