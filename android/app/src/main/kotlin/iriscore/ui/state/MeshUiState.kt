package iriscore.ui.state

import androidx.compose.runtime.Immutable

enum class MeshStatus { IDLE, STARTING, RUNNING, UNAVAILABLE }

@Immutable
data class InboxUiMessage(
    val senderId: String,
    val payloadUtf8: String,
    val priority: UByte,
    val receivedAtMs: Long,
    val pending: Boolean = false,
) {
    companion object {
        fun pending(recipientHex: String, payload: String, priority: UByte): InboxUiMessage =
            InboxUiMessage(
                senderId = recipientHex,
                payloadUtf8 = "[queued] $payload",
                priority = priority,
                receivedAtMs = System.currentTimeMillis(),
                pending = true,
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
        )
    }
}