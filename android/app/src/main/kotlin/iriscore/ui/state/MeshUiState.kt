package iriscore.ui.state

import androidx.compose.runtime.Immutable
import java.util.concurrent.atomic.AtomicLong

enum class MeshStatus { IDLE, STARTING, RUNNING, UNAVAILABLE }

/**
 * Delivery model for message rows.
 * RECEIVED = inbound. DELIVERED = engine accepted the send (best-effort local
 * evidence). QUEUED = spooled in RelayOutbox awaiting a future drain. FAILED =
 * send rejected with no outbox fallback. SENDING is kept for in-flight cases
 * where the engine call is still outstanding (should be very brief).
 */
enum class DeliveryStatus { RECEIVED, SENDING, DELIVERED, QUEUED, FAILED }

/**
 * HV-59: per-transport presence in the status line.
 * Derived from [FfiTransportDiag.state] in the engine snapshot; refreshed by the
 * ViewModel every 5 s while the mesh is RUNNING.
 */
@Immutable
data class TransportStatus(
    /** Short display label shown in the status chip ("BLE", "WD", "NET"). */
    val label: String,
    /** True when the transport's FfiTransportDiag.state == "Connected". */
    val connected: Boolean,
    /** Full state: Unavailable, Degraded, Available, or Connected. */
    val state: String,
)

@Immutable
data class InboxUiMessage(
    /**
     * Stable per-item identity for `LazyColumn`'s `key`. Derived fields are
     * unusable there: `senderId + receivedAtMs` collides for two messages that
     * arrive from one peer in the same millisecond (a duplicate key is a hard
     * Compose crash), and including the mutable `pending` flag makes the key
     * change when a queued message is delivered, so Compose discards and
     * rebuilds the row instead of updating it.
     */
    val uid: Long,
    /**
     * For inbound: the remote sender's peer-id hex.
     * For outbound: the recipient's peer-id hex (or [MeshRepository.BROADCAST_LABEL]).
     */
    val senderId: String,
    val payloadUtf8: String,
    val priority: UByte,
    val receivedAtMs: Long,
    val deliveryStatus: DeliveryStatus = DeliveryStatus.RECEIVED,
    /** True for messages this node sent; false for messages this node received. */
    val isOutbound: Boolean = false,
) {
    companion object {
        private val uids = AtomicLong(1L)

        fun nextUid(): Long = uids.getAndIncrement()

        fun received(
            senderId: String,
            payloadUtf8: String,
            priority: UByte,
            receivedAtMs: Long,
        ): InboxUiMessage = InboxUiMessage(
            uid = nextUid(),
            senderId = senderId,
            payloadUtf8 = payloadUtf8,
            priority = priority,
            receivedAtMs = receivedAtMs,
            isOutbound = false,
        )

        /** Engine accepted the send — best-effort local delivery evidence. */
        fun sent(recipientHex: String, payload: String, priority: UByte): InboxUiMessage =
            InboxUiMessage(
                uid = nextUid(),
                senderId = recipientHex,
                payloadUtf8 = payload,
                priority = priority,
                receivedAtMs = System.currentTimeMillis(),
                deliveryStatus = DeliveryStatus.DELIVERED,
                isOutbound = true,
            )

        /** Engine rejected the send; message spooled in RelayOutbox for a future drain. */
        fun pending(recipientHex: String, payload: String, priority: UByte): InboxUiMessage =
            InboxUiMessage(
                uid = nextUid(),
                senderId = recipientHex,
                payloadUtf8 = payload,
                priority = priority,
                receivedAtMs = System.currentTimeMillis(),
                deliveryStatus = DeliveryStatus.QUEUED,
                isOutbound = true,
            )
    }
}

@Immutable
data class MeshUiState(
    val nodeIdHex: String,
    val nodeIdShort: String,
    val identityBackend: String,
    val status: MeshStatus,
    val messages: List<InboxUiMessage>,
    val relayQueued: Int,
    /** Last user-visible failure (bad recipient, send rejected); null when clear. */
    val lastError: String? = null,
) {
    companion object {
        fun initial(
            nodeIdHex: String,
            identityBackend: String = "SOFTWARE",
        ): MeshUiState = MeshUiState(
            nodeIdHex = nodeIdHex,
            nodeIdShort = nodeIdHex.take(16),
            identityBackend = identityBackend,
            status = MeshStatus.IDLE,
            messages = emptyList(),
            relayQueued = 0,
            lastError = null,
        )
    }
}
