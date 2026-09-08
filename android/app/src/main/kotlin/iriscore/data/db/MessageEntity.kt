package iriscore.data.db

import androidx.room.Entity
import androidx.room.Index
import androidx.room.PrimaryKey
import iriscore.ui.state.DeliveryStatus
import iriscore.ui.state.InboxUiMessage

/**
 * Durable message row.
 *
 * For inbound rows: peerIdHex = the remote sender's peer-id hex.
 * For outbound rows: peerIdHex = the recipient's peer-id hex (or BROADCAST_LABEL).
 * The direction is stored in isOutbound so a send to peer A and a receive from
 * peer A are never confused.
 */
@Entity(
    tableName = "messages",
    indices = [
        Index("peerIdHex"),
        Index("timestampMs"),
    ],
)
data class MessageEntity(
    @PrimaryKey val uid: Long,
    val peerIdHex: String,
    val payloadUtf8: String,
    val priority: Int,
    val timestampMs: Long,
    /** [DeliveryStatus.name] — stored as text so new enum values don't need a migration. */
    val deliveryStatus: String,
    val isOutbound: Boolean,
) {
    fun toUiMessage(): InboxUiMessage = InboxUiMessage(
        uid = uid,
        senderId = peerIdHex,
        payloadUtf8 = payloadUtf8,
        priority = priority.toUByte(),
        receivedAtMs = timestampMs,
        deliveryStatus = runCatching { DeliveryStatus.valueOf(deliveryStatus) }
            .getOrDefault(DeliveryStatus.RECEIVED),
        isOutbound = isOutbound,
    )

    companion object {
        fun fromUiMessage(m: InboxUiMessage): MessageEntity = MessageEntity(
            uid = m.uid,
            peerIdHex = m.senderId,
            payloadUtf8 = m.payloadUtf8,
            priority = m.priority.toInt(),
            timestampMs = m.receivedAtMs,
            deliveryStatus = m.deliveryStatus.name,
            isOutbound = m.isOutbound,
        )
    }
}
