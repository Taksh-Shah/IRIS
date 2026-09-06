package iriscore.ui.state

import androidx.compose.runtime.Immutable
import java.util.concurrent.atomic.AtomicLong

enum class MeshStatus { IDLE, STARTING, RUNNING, UNAVAILABLE }

/**
 * HV-58: three-state outbound delivery model shown in the message row.
 * RECEIVED is the inbound case; SENDING/QUEUED/FAILED are the outbound states.
 * DELIVERED is reserved for when the FFI ACK drain is wired (HV-49 follow-up).
 */
enum class DeliveryStatus { RECEIVED, SENDING, QUEUED, FAILED }

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
    val senderId: String,
    val payloadUtf8: String,
    val priority: UByte,
    val receivedAtMs: Long,
    /** HV-58: delivery state for outbound messages; RECEIVED for inbound. */
    val deliveryStatus: DeliveryStatus = DeliveryStatus.RECEIVED,
) {
    companion object {
        private val uids = AtomicLong(1L)

        /** Next process-unique row id. */
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
        )

        fun pending(recipientHex: String, payload: String, priority: UByte): InboxUiMessage =
            InboxUiMessage(
                uid = nextUid(),
                senderId = recipientHex,
                payloadUtf8 = payload,
                priority = priority,
                receivedAtMs = System.currentTimeMillis(),
                deliveryStatus = DeliveryStatus.QUEUED,
            )

        /**
         * HW-2: `MeshRepository.send()` only ever added a row on the FAILURE
         * path (via [pending]) — a successful `engine.sendText()` returned
         * `true` and nothing else happened. On real hardware this meant a
         * sent message never appeared in the console at all: no error, no
         * history entry, indistinguishable from a message that silently
         * vanished, whether or not the transport actually delivered it.
         */
        fun sent(recipientHex: String, payload: String, priority: UByte): InboxUiMessage =
            InboxUiMessage(
                uid = nextUid(),
                senderId = recipientHex,
                payloadUtf8 = payload,
                priority = priority,
                receivedAtMs = System.currentTimeMillis(),
                deliveryStatus = DeliveryStatus.SENDING,
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
