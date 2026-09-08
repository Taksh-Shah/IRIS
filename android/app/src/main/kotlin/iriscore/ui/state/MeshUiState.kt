package iriscore.ui.state

import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.ui.res.stringResource
import iriscore.R
import java.util.concurrent.atomic.AtomicLong

enum class MeshStatus { IDLE, STARTING, RUNNING, UNAVAILABLE }

/**
 * WP11: plain-language mesh status for the fuller status surfaces
 * (accessibility description on the compact chip, banners, message info).
 * Not `permissionsGranted`-aware — the caller decides whether to show
 * [R.string.status_permission_needed] instead, since [MeshStatus] alone
 * cannot distinguish "denied permission" from "genuinely offline."
 */
@Composable
fun MeshStatus.plainLanguage(): String = stringResource(
    when (this) {
        MeshStatus.IDLE -> R.string.status_idle
        MeshStatus.STARTING -> R.string.status_starting
        MeshStatus.RUNNING -> R.string.status_running
        MeshStatus.UNAVAILABLE -> R.string.status_unavailable
    },
)

/**
 * Delivery model for message rows.
 * RECEIVED = inbound. DELIVERED = engine accepted the send (best-effort local
 * evidence) or a later ACK/outbox-drain confirmed it. QUEUED = spooled in
 * RelayOutbox awaiting a future drain. FAILED = the outbox gave up after
 * repeated drain attempts (see PendingMessageEntity.attempts) or the initial
 * send was rejected with no outbox fallback. EXPIRED = the message's TTL
 * elapsed while queued, with no delivery confirmation either way — distinct
 * from FAILED (a definite rejection) because the mesh genuinely does not know
 * what happened to it. SENDING is kept for in-flight cases where the engine
 * call is still outstanding (should be very brief).
 */
enum class DeliveryStatus { RECEIVED, SENDING, DELIVERED, QUEUED, FAILED, EXPIRED }

/** WP11: plain-language delivery status for chip semantics and message info. */
@Composable
fun DeliveryStatus.plainLanguage(): String = stringResource(
    when (this) {
        DeliveryStatus.RECEIVED -> R.string.delivery_received
        DeliveryStatus.SENDING -> R.string.delivery_sending
        DeliveryStatus.QUEUED -> R.string.delivery_queued
        DeliveryStatus.DELIVERED -> R.string.delivery_delivered
        DeliveryStatus.FAILED -> R.string.delivery_failed
        DeliveryStatus.EXPIRED -> R.string.delivery_expired
    },
)

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
